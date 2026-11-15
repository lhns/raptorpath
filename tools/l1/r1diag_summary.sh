#!/bin/bash
# §20: one line per diagnostic row from r1diag_run.sh's diag/ logs —
# goodput, class (fixed in the pre-registration), the server receiver's
# [RXSEC] split (last xfer line), [RDIAG] busy median, the tree witness
# (ibp_hit on the server [IOWN]) and the [LAT] nowait / over tokens.
#   bash r1diag_summary.sh <diag-dir>
set -u
D="${1:?diag dir}"
for c in "$D"/c1s-400-*-c.log; do
  t=$(basename "$c" -c.log); s="$D/$t-s.log"
  mbps=$(grep -a -o '"mbps":[0-9.]*' "$c" | tail -1 | cut -d: -f2)
  cls=AMBIGUOUS
  if [ -z "$mbps" ]; then cls=NO_DATA
  elif awk "BEGIN{exit !($mbps <= 260)}"; then cls=collapsed
  elif awk "BEGIN{exit !($mbps >= 330)}"; then cls=recovered; fi
  pipe=$(grep -a -c 'pipeline=window backend=Rlc hint=bulk' "$s")
  ibp=$(grep -a '\[IOWN\] phase=run side=server' "$s" | grep -c 'ibp_hit=')
  rx=$(grep -a '\[RXSEC\] phase=xfer side=server' "$s" | tail -1)
  tot=$(echo "$rx" | grep -o 'total_us=[0-9]*' | cut -d= -f2)
  book=$(echo "$rx" | grep -o 'book_us=[0-9]*' | cut -d= -f2)
  bpm=$(echo "$rx" | grep -o 'book_ns_pm=[0-9]*' | cut -d= -f2)
  tpm=$(echo "$rx" | grep -o 'total_ns_pm=[0-9]*' | cut -d= -f2)
  share=-
  [ -n "$tot" ] && [ "$tot" != 0 ] && share=$(awk "BEGIN{printf \"%.1f\", 100*$book/$tot}")
  busy=$(grep -a -o '\[RDIAG\] busy=[0-9]*' "$s" | cut -d= -f2 | sort -n | awk '{a[NR]=$1} END{if(NR) print a[int((NR+1)/2)]; else print "-"}')
  lat=$(grep -a '\[LAT\] site=receiver' "$s" | tail -1 | grep -o ' over=[0-9]*\| p0:n=[0-9]*\| nowait=[0-9]*' | tr -d '\n')
  echo "ROW $t mbps=${mbps:--} class=$cls pipe=$pipe ibp_lines=$ibp rxsec_total_ns_pm=${tpm:--} book_ns_pm=${bpm:--} book_share=$share rdiag_busy_med=$busy$lat"
done
