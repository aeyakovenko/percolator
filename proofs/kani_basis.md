How the table was obtained for this revision. The closure run is a full run on `cb78f1f0`. For the
other two runs, the shared machine's memory did not allow full runs, so on `cb78f1f0` every
harness that can reach a function changed or added since `d1fa39f6` was run (`proofs/kani_reach.py`
over the call graph, which can only over-approximate what a harness reaches; `proofs/changed_fns.py`
for what changed; outside `src/v16.rs` the two revisions differ only by added harnesses, and no
type changed): 45 harnesses with `--features fuzz` and 54 with the contracts command, all
verified, together with 201 further harnesses of the first command that were run before the
machine got busy. Every other harness runs only code identical to `d1fa39f6`'s, where it was
verified in full runs; the numbers above count both.
