# Runbook: daemon Mandor di VM lembur bersama

> Plan: `docs/plans/2026-10-23-002-feat-vm-overnight-mode-plan.md` (KTD-54 sampai KTD-70).
> Status: draf sebelum uji langsung U12; bagian bertanda **(menunggu U12)** dilengkapi saat uji.

Setiap developer menjalankan daemon `mandord` sendiri di VM sebagai user
Linux miliknya, dengan agent Claude Code sebagai user Linux terpisah
(`<dev>-agent`). Laptop tetap menjalankan daemon laptop; app tersambung ke
keduanya, ke VM lewat SSH.

Contoh di bawah memakai VM `instance-lembur` dan developer `handoko_ben`, user agent `handoko_ben-agent`, grup
`handoko_ben-mandor`, dan direktori grup `/srv/mandor/handoko_ben`.

## 1. Admin: user, grup, dan direktori (per developer)

```bash
# handoko_ben sudah ada di instance-lembur; untuk developer lain: sudo useradd -m -s /bin/bash <dev>
sudo useradd -m -s /bin/bash handoko_ben-agent
sudo groupadd handoko_ben-mandor
sudo usermod -aG handoko_ben-mandor handoko_ben
sudo usermod -aG handoko_ben-mandor handoko_ben-agent

# Clone, worktree, dan berkas run: daemon dan agent menulis, user lain tidak.
sudo install -d -o handoko_ben -g handoko_ben-mandor -m 2770 /srv/mandor/handoko_ben
sudo install -d -o handoko_ben -g handoko_ben-mandor -m 2770 /srv/mandor/handoko_ben/workspaces
sudo install -d -o handoko_ben -g handoko_ben-mandor -m 2770 /srv/mandor/handoko_ben/runs

# Biner mandord di ~/.local/bin bisa dieksekusi user agent; ~/.mandor tetap 0700.
sudo chmod 0711 /home/handoko_ben
sudo -u handoko_ben mkdir -p /home/handoko_ben/.local/bin
sudo chmod 0755 /home/handoko_ben/.local /home/handoko_ben/.local/bin

# Service kedua user tetap hidup tanpa sesi login.
sudo loginctl enable-linger handoko_ben
sudo loginctl enable-linger handoko_ben-agent
```

## 2. Admin: sudoers persis

Daemon menjalankan agent hanya lewat `mandord agent-exec`; tidak ada fallback
ke user daemon bila aturan ini salah (run gagal dengan pesan yang menunjuk
runbook ini).

```bash
sudo visudo -f /etc/sudoers.d/mandor-handoko_ben
```

```
handoko_ben ALL=(handoko_ben-agent) NOPASSWD: /home/handoko_ben/.local/bin/mandord agent-exec *
```

Cek dari user `handoko_ben`: `sudo -n -u handoko_ben-agent /home/handoko_ben/.local/bin/mandord agent-exec --reap 1 x`
harus keluar tanpa meminta password.

## 3. Admin: alat untuk agent

- Claude Code dipasang global (mis. `npm install -g @anthropic-ai/claude-code`)
  atau di home user agent; daemon memakai `MANDOR_CLAUDE_BIN` bila bukan `claude` di PATH.
- Review headless (KTD-64):
  ```bash
  sudo npm install -g @playwright/mcp@<versi yang dipin>
  sudo -iu handoko_ben-agent npx playwright install chromium
  ```
  Biner default `/usr/local/bin/mcp-server-playwright`; lain tempat: `MANDOR_PLAYWRIGHT_MCP`.
- Docker rootless untuk user agent bila repo memakai container:
  https://docs.docker.com/engine/security/rootless/ (socket di `/run/user/<uid agent>/docker.sock`).
- **Egress jaringan (disarankan):** pagar hook menolak staging, database
  jarak jauh, dan layanan pesan di tingkat teks perintah. Batas yang
  sebenarnya adalah aturan firewall keluar untuk user agent (mis. `nftables`
  dengan `meta skuid`), yang menolak host staging dan database tim.

## 4. Admin: rentang port dan Lark

- Beri setiap developer rentang port sendiri, mis. `handoko_ben` 5100–5199. Port app
  repo dan port callback login Lark harus di dalamnya; daemon menolak yang di luar.
- Daftarkan redirect URL `http://127.0.0.1:<port callback>/callback` developer
  itu di konsol app Lark (Security Settings → Redirect URLs).
- App Lark perlu kapabilitas bot dan izin kirim pesan ke user
  (`im:message:send_as_bot`) untuk DM laporan pagi.

## 5. Developer: pasang daemon peran VM

```bash
# Salin biner mandord versi yang sama dengan app laptop ke VM, lalu:
./mandord install
```

Lalu tambahkan konfigurasi peran VM sebagai drop-in systemd (tidak mengubah
unit yang dikelola `mandord install`):

```bash
mkdir -p ~/.config/systemd/user/mandord.service.d
cat > ~/.config/systemd/user/mandord.service.d/vm.conf <<'EOF'
[Service]
Environment=MANDOR_ROLE=vm
Environment=MANDOR_AGENT_USER=handoko_ben-agent
Environment=MANDOR_GROUP_DIR=/srv/mandor/handoko_ben
Environment=MANDOR_PORT_RANGE=5100-5199
EOF
systemctl --user daemon-reload
systemctl --user restart mandord
systemctl --user status mandord
```

## 6. Developer: login dari terminal SSH

```bash
gh auth login                               # sebagai handoko_ben: push dan PR oleh daemon
sudo -iu handoko_ben-agent claude                  # login Claude Code di user agent (perlu izin admin)
ssh -L 5150:127.0.0.1:5150 handoko_ben@instance-lembur          # dari laptop, lalu di VM:
mandord login lark
```

Sebelum login, isi Lark App daemon VM dari app laptop: **Settings → VM → Lark**
(app ID, secret, Base, tabel, dan port callback di rentang port Anda), setelah
link VM tersambung (langkah 7). Repo dan workspace VM juga didaftarkan dari
**Settings → VM → Repos/Workspaces** (path di `/srv/mandor/<dev>/workspaces`).
`mandord login lark` mencetak URL; buka di browser laptop.

Status ketiga login terlihat di tab **Overnight** app (atau RPC `status`).

## 7. Developer: sambungkan app

Di app laptop: **Settings → Overnight VM**, isi alias SSH atau `handoko_ben@instance-lembur`,
cocokkan fingerprint host key dengan yang diberikan admin, lalu **Trust and
connect**. Daemon laptop harus versi yang mendukung klaim per daemon
(protokol 1.1).

## 8. VM dimatikan atau restart

`mandord` dan service user agent start sendiri saat boot (linger). Antrean,
setelan, pemakaian siklus, dan worktree tetap ada. Kartu yang sedang
dikerjakan saat VM mati menjadi **Failed** ("the daemon restarted while this
run was active"); antrean berikutnya tetap berjalan. Di tab **Overnight**,
tekan **Queue again** pada kartu itu untuk melanjutkannya di VM pada branch
dan sesi yang sama, atau **Take over to laptop**. Bila VM mati lebih dari 30
menit, klaim kartunya menjadi basi dan kartu bisa diambil alih dari laptop.

## 9. Saat developer keluar

```bash
sudo -iu handoko_ben systemctl --user disable --now mandord
sudo loginctl disable-linger handoko_ben; sudo loginctl disable-linger handoko_ben-agent
sudo rm /etc/sudoers.d/mandor-handoko_ben
sudo -iu handoko_ben gh auth logout
sudo -iu handoko_ben-agent claude /logout          # atau hapus kredensial Claude user agent
sudo userdel -r handoko_ben-agent; sudo rm -rf /srv/mandor/handoko_ben
```

Cabut juga token Lark di konsol Lark bila perlu.

## Pemecahan masalah

| Gejala | Penyebab | Langkah |
|---|---|---|
| Run gagal "sudo: a password is required" | sudoers tidak cocok dengan jalur biner | Periksa langkah 2 |
| Daemon tidak start: "MANDOR_ROLE=vm needs ..." | Drop-in langkah 5 tidak lengkap | Lengkapi keempat variabel |
| "refusing to run git: the repo config sets ..." | Konfigurasi repo memuat kunci yang tidak dipercaya | `git config --unset <kunci>` di clone |
| Link VM di app "Host key verification failed" | Host key VM berubah | Hapus VM di app, konfirmasi fingerprint baru dengan admin |
