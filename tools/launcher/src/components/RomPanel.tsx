import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface RomStatus {
  installed: boolean;
  path: string | null;
}

interface RomCandidate {
  path: string;
  source: string;
  kind: "afs" | "iso";
  size: number;
  installed: boolean;
}

interface ImportResult {
  path: string;
  sha256: string;
  matches_expected: boolean;
}

interface Props {
  /** Called after a successful import with the new status and a summary line. */
  onImported: (status: RomStatus, summary: string) => void;
}

const mb = (n: number) => `${Math.round(n / (1024 * 1024))} MB`;

/** Shown while SF33RD.AFS is missing: lists discovered copies with Import. */
export function RomPanel({ onImported }: Props) {
  const [candidates, setCandidates] = useState<RomCandidate[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [manualPath, setManualPath] = useState("");

  const scan = async () => {
    setCandidates(null);
    try {
      const found = await invoke<RomCandidate[]>("find_rom_candidates");
      setCandidates(found.filter(c => !c.installed));
    } catch (err) {
      setMessage(`SCAN FAILED: ${err}`);
      setCandidates([]);
    }
  };

  useEffect(() => { scan(); }, []);

  const importRom = async (path: string) => {
    if (!path || busy) return;
    setBusy(path);
    setMessage("COPYING AND VERIFYING SF33RD.AFS...");
    try {
      const res = await invoke<ImportResult>("import_rom", { path });
      const summary = res.matches_expected
        ? "ROM IMPORTED — CHECKSUM VERIFIED"
        : `ROM IMPORTED — CHECKSUM ${res.sha256.slice(0, 12)}… IS NOT THE SUPPORTED USA DISC; IT MAY NOT WORK`;
      setMessage(summary);
      onImported(await invoke<RomStatus>("get_rom_status"), summary);
    } catch (err) {
      setMessage(`IMPORT FAILED: ${err}`);
    } finally {
      setBusy(null);
    }
  };

  const rowStyle = {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 16,
    padding: '8px 12px', marginBottom: 6, background: 'rgba(0,0,0,0.85)',
    border: '2px solid var(--accent-red)',
  } as const;
  const btnStyle = {
    padding: '6px 18px', fontSize: 14, fontFamily: 'var(--font-header)', fontStyle: 'italic', fontWeight: 800,
    background: 'transparent', color: '#fff', border: '2px solid var(--accent-yellow)', cursor: 'pointer',
    textTransform: 'uppercase', whiteSpace: 'nowrap',
  } as const;

  return (
    <div style={{ marginBottom: 24, background: 'rgba(0,0,0,0.6)', border: '2px solid var(--accent-yellow)', padding: 16 }}>
      <h3 style={{ fontSize: 28, color: 'var(--accent-yellow)', margin: '0 0 8px 0' }}>ROM REQUIRED: SF33RD.AFS</h3>
      <p style={{ color: 'rgba(255,255,255,0.8)', fontSize: 15, margin: '0 0 12px 0' }}>
        3SX needs SF33RD.AFS from your own PS2 copy (THIRD/SF33RD.AFS on the disc).
        Pick a copy found on this computer, or paste a path to the file or a disc image (.iso).
      </p>
      {candidates === null && <p style={{ color: '#fff' }}>SCANNING...</p>}
      {candidates?.length === 0 && <p style={{ color: '#fff', opacity: 0.7 }}>NO COPIES FOUND IN THE USUAL PLACES.</p>}
      {candidates?.map(c => (
        <div key={c.path} style={rowStyle}>
          <div style={{ minWidth: 0 }}>
            <div style={{ color: '#fff', fontFamily: 'var(--font-mono)', fontSize: 13, overflowWrap: 'anywhere' }}>{c.path}</div>
            <div style={{ color: 'var(--text-secondary)', fontSize: 12 }}>{c.source.toUpperCase()} · {c.kind.toUpperCase()} · {mb(c.size)}</div>
          </div>
          <button style={btnStyle} disabled={!!busy} onClick={() => importRom(c.path)}>
            {busy === c.path ? "IMPORTING..." : "IMPORT"}
          </button>
        </div>
      ))}
      <div style={{ ...rowStyle, borderColor: 'rgba(255,255,255,0.3)' }}>
        <input
          type="text"
          value={manualPath}
          onChange={e => setManualPath(e.target.value)}
          placeholder="/path/to/SF33RD.AFS or disc.iso"
          style={{ flex: 1, background: 'transparent', border: 'none', color: 'var(--accent-yellow)', fontFamily: 'var(--font-mono)', fontSize: 14 }}
        />
        <button style={btnStyle} disabled={!!busy || !manualPath} onClick={() => importRom(manualPath.trim())}>IMPORT</button>
        <button style={btnStyle} disabled={!!busy} onClick={scan}>RESCAN</button>
      </div>
      {message && <p style={{ color: '#fff', fontSize: 14, margin: '8px 0 0 0' }}>{message}</p>}
    </div>
  );
}
