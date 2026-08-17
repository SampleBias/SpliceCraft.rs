# Stage status

Update this ledger when a stage's acceptance list is fully green. Do not
check a box early.

- [x] 00 Bootstrap
- [x] 01 Core + sacred biology
- [x] 02 Persist + data safety
- [x] 03 File I/O
- [x] 04 Ratatui shell
- [x] 05 Map + sequence editor
- [x] 06 Library
- [x] 07 Enzymes + primers
- [x] 08 Cloning workbench
- [x] 09 Mutato + codon + synthesis
- [x] 10 Simulator + gels
- [ ] 11 Sequencing
- [ ] 12 Experiments + History UI
- [ ] 13 Search
- [ ] 14 Agent API + CLI
- [ ] 15 Satellite features
- [ ] 16 Parity gate

**Next:** stage 11 (`docs/stages/11-sequencing.md`).

Stage 10 notes: Helling–Goodman–Boyer mobility with form factors
(supercoiled 0.7× / nicked 1.4×). Gels persist as `gels.json` through
`safe_save_json`. PCR is exact-match (plus 3′ partial for 5′ flaps),
capped at 50 amplicons. Sticky-cut visualization remains deferred from
stage 05.

Stage 09 notes: codon silent-repair of internal Type IIS now runs on
coding inserts when a codon table is supplied. Mutato Tm is Wallace only
(no primer3). Sticky-cut visualization remains deferred from stage 05.

Stage 08 notes: codon silent-repair of internal Type IIS sites landed in
stage 09. Sticky-cut visualization remains deferred from stage 05.

Stage 05 notes: sticky-cut visualization (upstream/downstream tint on a
clicked Type IIS site) is deferred. Restriction overlay draws labeled
resite ticks; unique / 6+ / collection filters landed in stage 07.
