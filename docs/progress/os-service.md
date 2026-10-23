# Progres: daemon sebagai service OS

Plan: `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md`. Cakupan saat ini: U1–U4 (U5–U9 menyusul).

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `RunState::is_working` dan `any_working` (termasuk Preparing dan Publishing); `status` menambah `busy`, `managed_by`, `keep_awake` (opsional, daemon lama tidak mengirimnya); `--version` mencetak `mandord <versi>`. |
| U2 | selesai | `DrainGate` di `mandor-usecase` (std Mutex, tidak melewati `.await`; penghitung `in_flight`), dipegang `App` dan dibagi ke `RunService`; gerbang di semua titik mulai kerja (start, pesan, approve/continue plan, resume, takeover, publish, review, fix, `spawn_turn`); RPC `daemon.drain` dan `daemon.undrain`. |
| U3 | belum | |
| U4 | belum | |
