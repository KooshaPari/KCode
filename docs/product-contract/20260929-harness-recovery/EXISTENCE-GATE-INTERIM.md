# Existence-gate interim finding — KCode

Date: 2026-09-29. **Interim; not a retirement decision.**

Evidence has falsified multiple historical reasons for maintaining a deep fork: generic swarm coordination, harness API/SDK, elicitation/discovery, broad provider/subscription/account support, and runtime version/git identity are all substantially present in current upstream.

The current deep-fork thesis therefore remains OPEN but under strong pressure. Candidate surviving obligations are now:
1. exact responder-bound executable/PID evidence and integration with accepted MACE evidence policy;
2. specific behavioral patches in the 136 divergent commits that current upstream lacks;
3. native Windows/Pine behavior that matched tests show upstream cannot satisfy;
4. Pheno/Tracera/AgilePlus integrations whose placement in core is actually necessary rather than adapter-friendly.

The default architectural challenger is current upstream + thin owned overlay/adapters + upstream contributions + the minimum retained patch stack. Do not add new deep-fork features until the retained-patch ledger demonstrates why this challenger fails.

A future decision to retire/rebase KCode would be destructive/product-directional and is not taken by this program without the evidence gate and user authorization.
## Windows/Pine correction
Search of the frozen owned source did not surface a Pine/native-shell subsystem, while current upstream has substantial Windows-specific platform, shell, terminal-launch, setup and PowerShell handling. The user's Pine requirement remains accepted external ecosystem intent, but **it is not currently KCode differentiation**. To survive as a retained core patch, KCode must show an implemented behavior or required core hook that current upstream plus a Pine adapter cannot satisfy. Until then classify Pine as a consumer/integration boundary, not a reason for deep-fork ownership.
