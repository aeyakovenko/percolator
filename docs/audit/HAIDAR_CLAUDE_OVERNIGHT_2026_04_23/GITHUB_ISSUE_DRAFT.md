# [INFO] Build provenance SHA in README does not match deployed mainnet binary

**Repo**: `aeyakovenko/percolator-cli` — [README.md](README.md) "Build provenance" block for mainnet

## Summary

The SHA-256 in `README.md` for the mainnet BPF binary does not match the ELF payload of the deployed `ProgramData` account on mainnet-beta. Verification steps and actual hashes below. This does not affect funds (program is immutable, upgrade authority is burned), but it does break the stated reproducibility check and invites confusion when future auditors try to verify.

## Claim in README

```
BPF binary SHA-256:  3f78e2f279dc29aa373fca57cfc56a56d70b8a5e85a16e5a090a2f2d5d9efbcc
BPF binary size:     394,832 bytes ELF
percolator-prog:     06f86fb125525af81c0bfd19a295095dda102c07
percolator (engine): 3f55f871a3aa29d7b582fc2641d2106cbac0c32e
percolator-cli:      74e902f165dcac98c87eb80406a2a92a40cf8dc7
MAX_ACCOUNTS:        4096
```

And the stated verification:

> Verify locally: `solana program dump -u m BCGNFw6vDinWTF9AybAbi8vr69gx5nk5w8o2vEWgpsiw /tmp/mainnet.so` then `head -c 394832 /tmp/mainnet.so | sha256sum` — must output the SHA above.

## Observed

`solana program show` on mainnet reports:

```
Program Id: BCGNFw6vDinWTF9AybAbi8vr69gx5nk5w8o2vEWgpsiw
Owner: BPFLoaderUpgradeab1e11111111111111111111111
ProgramData Address: 73Edg9QUV6o8HhY8EWfZDhUqbgzwNkUvahr5vpof2EmR
Authority: none
Last Deployed In Slot: 414977364
Data Length: 395368 (0x60868) bytes
Balance: 2.75296536 SOL
```

`Data Length: 395368` ≠ `394832`. A plain `solana program dump` on the upgradeable-loader layout returns the ELF *with* the 45-byte ProgramData header stripped, so the "ELF bytes" we should hash are 395,368 — 536 more than the README claims.

Running the exact command from the README:

```
$ solana program dump -u m BCGNFw6vDinWTF9AybAbi8vr69gx5nk5w8o2vEWgpsiw /tmp/mainnet.so
$ head -c 394832 /tmp/mainnet.so | sha256sum
fe97296cba72b9225feb7615f5a4e1aff5ac932485f00d979ea8d3be09a3dc40  -

# For completeness, the full ELF (all 395368 bytes):
$ sha256sum /tmp/mainnet.so
502088e9cf5e1b38cccd31bbab2df18d4958712fb9456d48669241aaddf4cc93  /tmp/mainnet.so
```

Neither matches `3f78e2f279dc29aa373fca57cfc56a56d70b8a5e85a16e5a090a2f2d5d9efbcc`. I also tried offsets `0/45/16` crossed with lengths `394832/395368/394786` — no match.

The 536-byte discrepancy is NOT trailing zero padding: bytes 394832..395368 contain ELF section-header content (`2d 00 00 00 06 00 00 00 …`), i.e. real program-data bytes that a truncated hash would drop.

## What this is *not*

- Not a theft vector. The program is deployed under BPF upgradeable loader with `Authority: none`, so no upgrade can change what runs on-chain.
- Not evidence of tampering. The more likely explanation is that the README block was prepared before the final deploy step, or it was generated on a platform with a subtly different ELF layout.

## What this *is*

- A broken reproducibility check. Any third-party auditor following README instructions verbatim gets a mismatch and is left wondering whether the deployed binary matches `06f86fb`.
- A surface for supply-chain confusion if the repo is ever forked or re-used as a template.

## Suggested fix

Either:
1. Regenerate the provenance block from the actual on-chain bytes and update `README.md` (simplest):
   ```
   BPF binary SHA-256 (full ELF):    502088e9cf5e1b38cccd31bbab2df18d4958712fb9456d48669241aaddf4cc93
   BPF binary SHA-256 (first 394832 bytes):  fe97296cba72b9225feb7615f5a4e1aff5ac932485f00d979ea8d3be09a3dc40
   BPF binary size:                   395,368 bytes ELF (not 394,832)
   ```
   and change the verification recipe to `sha256sum /tmp/mainnet.so` (full file).
2. Publish the exact toolchain + docker image used for the mainnet build so anyone can reproduce `3f78e2f2…` deterministically.

## Reproduction

```bash
solana --version
# solana-cli 3.0.8 (src:b4d1c774; feat:3604001754, client:Agave)

solana program dump -u mainnet-beta \
  BCGNFw6vDinWTF9AybAbi8vr69gx5nk5w8o2vEWgpsiw \
  /tmp/mainnet.so

wc -c /tmp/mainnet.so
# 395368 /tmp/mainnet.so

head -c 394832 /tmp/mainnet.so | sha256sum
# fe97296cba72b9225feb7615f5a4e1aff5ac932485f00d979ea8d3be09a3dc40  -

sha256sum /tmp/mainnet.so
# 502088e9cf5e1b38cccd31bbab2df18d4958712fb9456d48669241aaddf4cc93  /tmp/mainnet.so
```

## Severity

INFO / documentation. No funds at risk.
