# Assumptions and evidence to sign off (layer Y, engine 71076137, program e1295564)

DRAFT for the owner's sign-off. These are not proved and cannot be: they are what the proofs assume about the
platform and the tools, and the evidence that stands in for properties the provers do not state (compute bounds,
termination, the reading of each statement against the charter). Each line is the ledger's statement of the
piece; signing it off closes the piece as an assumption, recorded as such in the roadmap's final check.

## Y.runtime

assumed: transaction atomicity (a failed instruction changes nothing), account ownership and signer checks, rent and reallocation rules, the Clock sysvar (the elapsed slots of the accruals and the price clamps come from it; seven handlers would fall back to the instruction's slot argument if the read failed, which it does not on-chain), the compute meter

How it stands: written down and signed off; not a proof; nothing recorded yet.

## Y.spl

assumed: a transfer moves exactly the amount between the named accounts or fails; mint and owner fields are as the token program wrote them

How it stands: written down and signed off; not a proof; nothing recorded yet.

## Y.oracle

assumed: the layouts the program parses (Pyth, Switchboard, Chainlink) are the publishers' layouts. Nothing is assumed about the values: what an oracle reports and what any authority signs are adversarial inputs. The charter already bounds what they can do to a user (INV-005 with its role matrix, INV-014, INV-045, INV-046, INV-064, INV-018, the isolation invariants and the exit rule), and those invariants are proved with the oracle and the authorities quantified over

How it stands: written down and signed off; not a proof; nothing recorded yet.

## Y.tool

trusted: its model of Rust and of the core library, with the library behaviours each run summary lists as assumed, and the bound on object sizes (2^56 bytes)

How it stands: written down and signed off; not a proof; nothing recorded yet.

## Y.compute

evidence, not proof: every instruction fits the compute limit at the maximum shape (measured by the program's max-shape tests)

How it stands: written down and signed off; not a proof; nothing recorded yet.

## Y.terminates

evidence, not proof: every loop is bounded by a compile-time constant or by an input length that the compute meter bounds (Aristotle proves no termination)

How it stands: written down and signed off; not a proof; nothing recorded yet.

## Y.statements

an independent reader has checked that each formal statement says what the charter invariant means; a proof of the wrong statement proves nothing

How it stands: written down and signed off; not a proof; nothing recorded yet.

## Sign-off

Owner: ________   Date: ________   Revision: engine ________ / program ________
