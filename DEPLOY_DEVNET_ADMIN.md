# Deploy Percolator to Solana Devnet - Admin Guide

The Solana BPF compilation requires admin privileges. Follow this guide to deploy successfully.

## ✅ Status Update

Your account is **ready for devnet**:
- ✅ Connected to devnet: `https://api.devnet.solana.com`
- ✅ Account: `J5v6JpZsEEursd6UbbybgasdsV4tGKV8kmQvkAZucUHr`
- ✅ Balance: **5 SOL** (airdrop successful! 🎉)
- ✅ Dashboard `.env.local` configured

---

## 🔧 Build & Deploy Programs

### Step 1: Open PowerShell as Administrator

1. Press `Win + X`
2. Click "Windows PowerShell (Admin)" or "Terminal (Admin)"
3. Click "Yes" when prompted

### Step 2: Set Environment Variables

```powershell
# Set HOME variable for cargo-build-sbf
$env:HOME="$env:USERPROFILE"

# Verify it's set
Write-Host "HOME is set to: $env:HOME"
```

### Step 3: Build for SBF (BPF)

```powershell
cd C:\Users\Rey\Desktop\percolator-fork

# Build all programs for Solana BPF target
cargo build-sbf

# This will:
# 1. Download Solana toolchain
# 2. Compile programs to .so files
# 3. Output to: target/sbf-solana-solana/release/
```

Expected output:
```
Finished `sbf-release` profile [optimized] target(s) in X.XXs
```

### Step 4: Deploy to Devnet

```powershell
# Ensure you're on devnet
solana config set --url https://api.devnet.solana.com

# Deploy Router program
solana program deploy target/sbf-solana-solana/release/percolator_router.so

# Deploy Slab program
solana program deploy target/sbf-solana-solana/release/percolator_slab.so

# Verify programs are deployed
solana program show RoutR1VdCpHqj89WEMJhb6TkGT9cPfr1rVjhM3e2YQr
solana program show SLabZ6PsDLh2X6HzEoqxFDMqCVcJXDKCNEYuPzUvGPk
```

### Step 5: Get Your Program IDs

The output will show actual program IDs. **Note them down!**

If different from defaults, update your `.env.local`:

File: `C:\Users\Rey\Desktop\perp\.env.local`

```env
NEXT_PUBLIC_ROUTER_PROGRAM=<YOUR_NEW_ROUTER_ID>
NEXT_PUBLIC_SLAB_PROGRAM=<YOUR_NEW_SLAB_ID>
```

---

## 📊 What's Ready Now

| Component | Status | Details |
|-----------|--------|---------|
| Devnet RPC | ✅ Connected | `https://api.devnet.solana.com` |
| Devnet SOL | ✅ 5 SOL | Account funded |
| Dashboard | ✅ Deployed | Live on Vercel |
| `.env.local` | ✅ Created | Ready for deployment |
| Programs | ⏳ Pending | Awaiting build (needs admin) |

---

## 🚀 Quick Deploy Command (Run as Admin)

Copy and run this in **PowerShell (Admin)**:

```powershell
$env:HOME="$env:USERPROFILE"
cd C:\Users\Rey\Desktop\percolator-fork
cargo build-sbf
solana program deploy target/sbf-solana-solana/release/percolator_router.so
solana program deploy target/sbf-solana-solana/release/percolator_slab.so
solana program show RoutR1VdCpHqj89WEMJhb6TkGT9cPfr1rVjhM3e2YQr
solana program show SLabZ6PsDLh2X6HzEoqxFDMqCVcJXDKCNEYuPzUvGPk
```

---

## 🐛 Troubleshooting

### "A required privilege is not held by the client"
- **Solution**: Run PowerShell as Administrator (Admin mode)
- **How**: Win + X → Terminal (Admin) → Yes

### "Can't get home directory path"
- **Solution**: Run this before build-sbf:
  ```powershell
  $env:HOME="$env:USERPROFILE"
  ```

### "Failed to download Solana toolchain"
- **Solution**: Check internet connection, retry build-sbf
- **Alternative**: Install Solana Platform Tools manually from https://docs.solana.com/cli/install

### Programs show "not deployed"
- **Check**: Verify you're on devnet: `solana config get`
- **Fix**: `solana config set --url https://api.devnet.solana.com`

---

## 📚 Next Steps After Deployment

1. **Verify programs** - Run `solana program show` for each program
2. **Update dashboard** - Copy program IDs to `.env.local` if different
3. **Push changes** - Commit and push to GitHub
4. **Redeploy dashboard** - Vercel will auto-rebuild
5. **Test trading** - Connect wallet and place test orders

---

## 💡 Key Points

- Admin privileges **only needed for `cargo build-sbf`**
- Once built, normal privileges work for deployment
- SBF compilation may take 2-5 minutes first time
- Programs deploy to devnet instantly (~10 seconds)
- Dashboard already configured for devnet in `.env.local`

---

**Ready to deploy? Open PowerShell as Admin and follow the Quick Deploy Command above!** 🚀
