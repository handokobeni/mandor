// Link kedua ke daemon VM (U9 plan VM).

/**
 * Daemon laptop mendukung klaim per daemon (protokol 1.1, KTD-55). Daemon lama
 * menimpa klaim VM milik developer yang sama, jadi link VM tidak diaktifkan.
 */
export function supportsVmLink(protocol: string | undefined): boolean {
  const [major, minor] = (protocol ?? '').split('.').map((p) => Number.parseInt(p, 10));
  return major === 1 && Number.isFinite(minor) && minor >= 1;
}
