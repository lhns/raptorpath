//! Streaming FEC interface — sends source symbols immediately,
//! then streams repair symbols based on controller demand.

use super::codec::{Encoder, EncodingParams, WireSymbol};
use tokio::sync::mpsc;

/// A FEC stream that first yields all source symbols, then generates
/// repair symbols on demand from the controller.
pub struct FecStream {
    encoder: Encoder,
    source_emitted: bool,
}

impl FecStream {
    pub fn new(data: &[u8], params: EncodingParams) -> Self {
        Self {
            encoder: Encoder::new(data, params),
            source_emitted: false,
        }
    }

    /// Get source symbols (call once, first).
    pub fn take_source_symbols(&mut self) -> Vec<WireSymbol> {
        self.source_emitted = true;
        self.encoder.source_symbols()
    }

    /// Generate additional repair symbols on demand.
    /// Can be called multiple times — fountain codes generate unlimited repair.
    pub fn generate_repair(&self, count: u32) -> Vec<WireSymbol> {
        self.encoder.repair_symbols(count)
    }
}

/// Background repair symbol generator that feeds into a channel.
pub struct RepairStream;

impl RepairStream {
    /// Spawn a task that generates repair symbols and sends them to the channel.
    /// The controller can request more by sending a count on `demand_rx`.
    pub fn spawn(
        encoder: Encoder,
        mut demand_rx: mpsc::Receiver<u32>,
        symbol_tx: mpsc::Sender<Vec<WireSymbol>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(count) = demand_rx.recv().await {
                let repair = encoder.repair_symbols(count);
                if symbol_tx.send(repair).await.is_err() {
                    break;
                }
            }
        })
    }
}
