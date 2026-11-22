How the table was obtained for this revision. The runs were made on the batch's commit before it
was rebased onto a documentation-only commit of master; `src/`, `tests/` and the Cargo files of that
commit are identical to `80f8f0b4`'s. Every harness that can reach a function changed or added since
`cb78f1f0` was run on that code (`proofs/kani_reach.py` over the
call graph, which can only over-approximate what a harness reaches; `proofs/changed_fns.py` for what
changed; outside `src/v16.rs` the revisions differ only by added harnesses and a test file, and no
type changed): 110 harnesses with `--features fuzz` and 123 with the contracts command (21 of them
new), all verified. The affected harnesses of the closure run are harnesses of the first command on
identical code, apart from the one without a verdict, as before. The change to config admission is
not counted as reaching a harness: no harness gives the validation a public B chunk above
`MAX_VAULT_TVL`. Every other harness runs only code identical to `cb78f1f0`'s, where it verified;
the numbers above count both.
