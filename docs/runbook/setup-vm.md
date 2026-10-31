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
  Contoh di runbook ini: callback Lark `handoko_ben` = 5199.
- Tambahkan redirect URL `http://127.0.0.1:5199/callback` di konsol app Lark
  (Security Settings → Redirect URLs). `http://127.0.0.1:53682/callback` milik
  daemon laptop tetap dipertahankan. Host tetap `127.0.0.1` karena callback
  dibuka browser laptop dan diteruskan ke VM lewat `ssh -L`.
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

## 6. Developer: akses SSH dari laptop

App dan login Lark memakai `ssh` sungguhan dari laptop (WSL); SSH-in-browser
Google Cloud tidak bisa meneruskan port. `instance-lembur` adalah nama
instance GCP, bukan nama host yang bisa dijangkau, jadi buat alias `lembur`
di `~/.ssh/config` WSL laptop dengan salah satu cara:

```sshconfig
# A. VM punya IP eksternal dan kunci SSH Anda sudah didaftarkan
#    (OS Login: `gcloud compute os-login ssh-keys add --key-file ~/.ssh/id_ed25519.pub`,
#    atau admin menambahkannya di metadata instance).
Host lembur
    HostName <IP eksternal instance-lembur>
    User handoko_ben
    IdentityFile ~/.ssh/id_ed25519

# B. Tanpa IP eksternal: lewat IAP dengan gcloud (zona sesuai instance).
Host lembur
    User handoko_ben
    IdentityFile ~/.ssh/google_compute_engine
    ProxyCommand gcloud compute start-iap-tunnel instance-lembur 22 --listen-on-stdin --zone <zona>
```

Cek: `ssh lembur hostname` harus mencetak `instance-lembur` tanpa meminta
password. Pakai satu blok `Host lembur` saja; dua blok dengan nama sama
digabung ssh (mis. `HostName` dari cara A ikut dipakai bersama `ProxyCommand`
cara B). **Check host key** di app membaca kunci lewat `ssh` dengan config ini,
jadi berjalan juga lewat IAP.

Akun yang dipakai `gcloud compute ssh` bisa berbeda dengan akun
SSH-in-browser (mis. `handokobeni` vs `handoko_ben`). Daftarkan public key
laptop (`~/.ssh/google_compute_engine.pub`) di `~/.ssh/authorized_keys` milik
user developer (`handoko_ben`) dari SSH-in-browser, lalu cek `ssh lembur whoami`.

## 7. Developer: sambungkan app, isi Lark dan repo VM

1. Di app laptop: **Settings → Overnight VM**, isi `lembur`, cocokkan
   fingerprint host key dengan yang diberikan admin, lalu **Trust and
   connect**. Daemon laptop harus versi yang mendukung klaim per daemon
   (protokol 1.1).
2. **Settings → VM → Lark**: app ID, secret, Base, tabel, dan **Callback port
   5199** (di rentang port Anda dan terdaftar di app Lark, bagian 4).
3. **Settings → VM → Repos/Workspaces**: daftarkan repo dengan path di
   `/srv/mandor/handoko_ben/workspaces`; port app repo juga di rentang
   5100–5199. Untuk review headless, aktifkan **Restart the app before
   reviews** dan **Setup command prepares a test login**: daemon VM
   menyalakan app run sendiri sebelum review dan menghentikan app kartu
   sebelumnya (kartu lembur dikerjakan satu per satu).

4. **Settings → Overnight VM → Design sources on the VM** (opsional):
   daftarkan folder desain prototype yang ada di VM. Clone repo desain sebagai
   user developer (bukan user agent) di direktori grup, mis.:

   ```bash
   # Sebagai handoko_ben di VM: milik developer, bisa dibaca grup, tidak bisa ditulis agent.
   install -d -m 2750 /srv/mandor/handoko_ben/design
   git clone https://github.com/<org>/kelola-mock.git /srv/mandor/handoko_ben/design/kelola-mock
   chmod -R g-w /srv/mandor/handoko_ben/design/kelola-mock
   ```

   Sebelum kartu dimulai, daemon VM memperbarui folder yang berupa repo git
   (fetch lalu fast-forward saja, dengan token `gh` daemon dan konfigurasi git
   yang diperketat). Folder yang `.git`-nya bukan milik user developer tidak
   di-pull. Pull yang gagal tidak menghentikan kartu; laporan pagi mencatatnya.
5. **Plan siang, lembur malam**: di panel plan yang menunggu persetujuan,
   **Approve & queue for VM** memilih repo dan sumber desain di VM. Laptop
   mem-push branch kartu, VM mengantre kartu, lalu klaim pindah ke VM dan run
   laptop ditutup. Baris antrean **Claim not moved** berarti langkah terakhir
   gagal: **Retry handing over** atau **Remove from VM queue**. Kartu Planned
   yang masih menunggu bisa ditarik kembali dengan **Take over to laptop**;
   plannya kembali menunggu persetujuan di laptop.

## 8. Developer: login

```bash
# Di VM (SSH-in-browser atau ssh lembur):
gh auth login                                  # sebagai handoko_ben: push dan PR oleh daemon
sudo -iu handoko_ben-agent claude              # login Claude Code di user agent (perlu izin admin)

```

Login Lark daemon VM (perintah pertama di laptop, kedua di VM):

```bash
# 1. Di laptop (WSL): masuk ke VM sambil meneruskan port callback 5199.
ssh -L 5199:127.0.0.1:5199 lembur

# 2. Di prompt VM yang terbuka dari perintah di atas:
mandord login lark          # atau ~/.local/bin/mandord login lark
```

3. Buka URL yang dicetak di **browser laptop** dan login. Callback ke
   `127.0.0.1:5199` diteruskan ke VM; terminal VM menampilkan "Logged in to
   Lark". Jangan jalankan `mandord login lark` di laptop: itu login daemon
   laptop, bukan daemon VM.

Status ketiga login terlihat di tab **Overnight** app (atau RPC `status`).

## 9. VM dimatikan atau restart

`mandord` dan service user agent start sendiri saat boot (linger). Antrean,
setelan, pemakaian siklus, dan worktree tetap ada. Kartu yang sedang
dikerjakan saat VM mati menjadi **Failed** ("the daemon restarted while this
run was active"); antrean berikutnya tetap berjalan. Di tab **Overnight**,
tekan **Queue again** pada kartu itu untuk melanjutkannya di VM pada branch
dan sesi yang sama, atau **Take over to laptop**. Bila VM mati lebih dari 30
menit, klaim kartunya menjadi basi dan kartu bisa diambil alih dari laptop.

## 10. Saat developer keluar

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
| Tab Overnight "VM offline" dengan "Connection closed by UNKNOWN port 65535" (akses lewat IAP) | Login `gcloud` di laptop kedaluwarsa (akun organisasi Google meminta login ulang berkala); `ssh` dari app tidak bisa menampilkan prompt | `gcloud auth login <akun>` di WSL, lalu cek `ssh lembur hostname`. Daemon VM tetap bekerja; hanya akses dari laptop yang terputus |
