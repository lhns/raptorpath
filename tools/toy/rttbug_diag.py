import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.argv = [sys.argv[0], "none"]
import rttbug, batch3d
class D(rttbug.CSim):
    def on_report(self, now, rep):
        n0 = self.cnt["rfc_bad"]; sr0 = self.srtt
        super().on_report(now, rep)
        if self.cnt["rfc_bad"] > n0:
            s = rep[3]
            print("t=%.4f rep_made=%.4f seq=%d sent_t=%.4f sample=%.4f srtt_before=%s srtt_after=%.4f F_rx=%d" % (now, rep[0], s, self.sent_t[s], now-self.sent_t[s], sr0, self.srtt, self.F_rx))
for n in ("CL-long.G-T0p4-H0", "OV-long.G-T0p4-H0"):
    print(n); D(batch3d.JOBS[n][1](0)).run()
