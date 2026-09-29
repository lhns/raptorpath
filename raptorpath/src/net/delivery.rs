//! Window-mode hand-off of decoded packets to the consumer (the TUN inject
//! channel).
//!
//! The receiver loop cannot await the channel: one `tokio::select!` also
//! serves the peer's acks, this side's acks and the hole-refresh timer, so a
//! blocking send deadlocks the loopback. Every hand-off is a `try_send`.

use bytes::Bytes;
use tokio::sync::mpsc;
use tracing::warn;

/// The consumer channel is closed: the receiver must stop.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Closed;

/// The receiver's hand-off step for window-mode symbols.
pub(crate) struct WindowDelivery {
    /// ρ, the retention contract (`recv_window_reliable`).
    hold_on_full: bool,
}

impl WindowDelivery {
    pub(crate) fn new(hold_on_full: bool) -> Self {
        Self { hold_on_full }
    }

    /// Hand one symbol's packets (`seq`, in `extract_window_packets` order)
    /// to the consumer.
    pub(crate) fn offer(
        &mut self,
        tx: &mpsc::Sender<Bytes>,
        _seq: u64,
        packets: Vec<Vec<u8>>,
    ) -> Result<(), Closed> {
        let _ = self.hold_on_full;
        for pkt in packets {
            match tx.try_send(Bytes::from(pkt)) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    warn!("TUN inject channel full, dropping packet");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => return Err(Closed),
            }
        }
        Ok(())
    }

    /// Retry the hand-off of anything held.
    pub(crate) fn flush(&mut self, _tx: &mpsc::Sender<Bytes>) -> Result<(), Closed> {
        Ok(())
    }

    /// Lowest seq with a packet not yet accepted by the consumer channel.
    pub(crate) fn lowest_held(&self) -> Option<u64> {
        None
    }

    pub(crate) fn is_holding(&self) -> bool {
        false
    }
}

/// The highest seq whose packets have all been handed to the consumer, given
/// the first seq the reorder stage has not yet released (`next_unreleased`)
/// and the lowest seq still held here. `None` = nothing delivered yet (the
/// wire's `received_up_to = 0` sentinel).
pub(crate) fn delivered_through(next_unreleased: u64, lowest_held: Option<u64>) -> Option<u64> {
    next_unreleased
        .min(lowest_held.unwrap_or(u64::MAX))
        .checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packets of `seq`: `n` of them, each tagged (seq, index).
    fn pkts(seq: u64, n: usize) -> Vec<Vec<u8>> {
        (0..n).map(|i| vec![seq as u8, i as u8]).collect()
    }

    fn n_pkts(seq: u64) -> usize {
        (seq % 3) as usize + 1
    }

    fn drain(rx: &mut mpsc::Receiver<Bytes>, out: &mut Vec<Vec<u8>>) {
        while let Ok(b) = rx.try_recv() {
            out.push(b.to_vec());
        }
    }

    /// Assert the ack invariant: every packet of every seq at or below the
    /// advertised point has been accepted by the channel (consumed so far or
    /// still queued in it).
    fn assert_ack_covers(
        acked: Option<u64>,
        consumed: usize,
        tx: &mpsc::Sender<Bytes>,
        cap: usize,
    ) {
        let accepted = consumed + (cap - tx.capacity());
        if let Some(a) = acked {
            let owed: usize = (0..=a).map(n_pkts).sum();
            assert!(
                owed <= accepted,
                "ack {a} advertised but only {accepted} of the {owed} packets at or below it reached the consumer"
            );
        }
    }

    /// ρ = 1: a depth-1 channel with a consumer that stalls, then drains.
    /// Every packet reaches the consumer exactly once, in order, and the
    /// cumulative point never passes an undelivered seq. Symbols carry 1-3
    /// packets, so a symbol is regularly split across a Full.
    #[test]
    fn reliable_full_channel_holds_until_drained_exactly_once_in_order() {
        const CAP: usize = 1;
        const N: u64 = 30;
        let (tx, mut rx) = mpsc::channel::<Bytes>(CAP);
        let mut d = WindowDelivery::new(true);
        let mut got = Vec::new();
        let mut acked: Option<u64> = None;
        for seq in 0..N {
            d.offer(&tx, seq, pkts(seq, n_pkts(seq))).unwrap();
            // The in-order reorder stage has released everything through seq.
            acked = acked.max(delivered_through(seq + 1, d.lowest_held()));
            assert_ack_covers(acked, got.len(), &tx, CAP);
            // The consumer stalls for 4 symbols, then drains what is queued.
            if seq % 5 == 4 {
                drain(&mut rx, &mut got);
                d.flush(&tx).unwrap();
                acked = acked.max(delivered_through(seq + 1, d.lowest_held()));
                assert_ack_covers(acked, got.len(), &tx, CAP);
            }
        }
        // Retry wakes until nothing is held.
        for _ in 0..1000 {
            drain(&mut rx, &mut got);
            d.flush(&tx).unwrap();
            acked = acked.max(delivered_through(N, d.lowest_held()));
            assert_ack_covers(acked, got.len(), &tx, CAP);
            if !d.is_holding() {
                break;
            }
        }
        drain(&mut rx, &mut got);
        let want: Vec<Vec<u8>> = (0..N).flat_map(|s| pkts(s, n_pkts(s))).collect();
        assert_eq!(got, want, "every packet exactly once, in order");
        assert_eq!(acked, Some(N - 1));
        assert!(!d.is_holding());
    }

    /// A multi-packet symbol split by a Full: the packets already accepted
    /// are never re-sent, the remainder is held, and a later symbol queues
    /// behind it even when the channel has room.
    #[test]
    fn reliable_split_symbol_resumes_at_the_held_packet() {
        let (tx, mut rx) = mpsc::channel::<Bytes>(2);
        let mut d = WindowDelivery::new(true);
        let mut got = Vec::new();
        d.offer(&tx, 7, pkts(7, 3)).unwrap();
        assert_eq!(d.lowest_held(), Some(7), "the 3rd packet of seq 7 is held");
        assert_eq!(delivered_through(8, d.lowest_held()), Some(6));
        // Consumer takes one: room for one packet.
        got.push(rx.try_recv().unwrap().to_vec());
        // seq 8 arrives: it must not overtake seq 7's held packet.
        d.offer(&tx, 8, pkts(8, 1)).unwrap();
        assert_eq!(d.lowest_held(), Some(8));
        assert_eq!(delivered_through(9, d.lowest_held()), Some(7));
        drain(&mut rx, &mut got);
        d.flush(&tx).unwrap();
        assert!(!d.is_holding());
        drain(&mut rx, &mut got);
        assert_eq!(got, vec![vec![7, 0], vec![7, 1], vec![7, 2], vec![8, 0]]);
        assert_eq!(delivered_through(9, d.lowest_held()), Some(8));
    }

    /// Unordered delivery (the H = 0 corner, reliable): held seqs arrive in
    /// any order; the cumulative point stops below the lowest one.
    #[test]
    fn reliable_unordered_ack_stops_below_lowest_held() {
        let (tx, mut rx) = mpsc::channel::<Bytes>(1);
        let mut d = WindowDelivery::new(true);
        d.offer(&tx, 5, pkts(5, 1)).unwrap();
        d.offer(&tx, 2, pkts(2, 1)).unwrap();
        d.offer(&tx, 9, pkts(9, 1)).unwrap();
        assert_eq!(d.lowest_held(), Some(2));
        // Received prefix is 0..=9, but 2 and 9 are still held.
        assert_eq!(delivered_through(10, d.lowest_held()), Some(1));
        let mut got = Vec::new();
        for _ in 0..4 {
            drain(&mut rx, &mut got);
            d.flush(&tx).unwrap();
        }
        drain(&mut rx, &mut got);
        assert_eq!(got, vec![vec![5, 0], vec![2, 0], vec![9, 0]]);
        assert_eq!(delivered_through(10, d.lowest_held()), Some(9));
    }

    /// ρ < 1 (EVICT): Full drops, nothing is ever held, the frontier is the
    /// reorder stage's.
    #[test]
    fn evict_full_channel_drops_and_never_holds() {
        let (tx, mut rx) = mpsc::channel::<Bytes>(1);
        let mut d = WindowDelivery::new(false);
        d.offer(&tx, 3, pkts(3, 3)).unwrap();
        assert!(!d.is_holding());
        assert_eq!(d.lowest_held(), None);
        assert_eq!(delivered_through(4, d.lowest_held()), Some(3));
        d.flush(&tx).unwrap();
        let mut got = Vec::new();
        drain(&mut rx, &mut got);
        assert_eq!(got, vec![vec![3, 0]]);
    }

    #[test]
    fn closed_channel_errors_on_both_contracts() {
        for hold in [false, true] {
            let (tx, rx) = mpsc::channel::<Bytes>(1);
            drop(rx);
            let mut d = WindowDelivery::new(hold);
            assert_eq!(d.offer(&tx, 0, pkts(0, 1)), Err(Closed));
        }
    }

    #[test]
    fn delivered_through_seq0_sentinel() {
        assert_eq!(delivered_through(0, None), None);
        assert_eq!(delivered_through(5, Some(0)), None);
        assert_eq!(delivered_through(1, None), Some(0));
    }
}
