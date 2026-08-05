// Barre Play Mode : build + lancement PCSX-Redux, puis pilotage par
// l'API web de l'émulateur (statut, pause/reprise, reset).

import { useEffect, useRef, useState } from "react";
import { api } from "./bridge";

export const PLAY_PORT = 8080;
const PORT = PLAY_PORT;

type PlayState = "idle" | "building" | "launched" | "error";

export function PlayBar({
  projectDir,
  onRunningChange,
}: {
  projectDir: string | null;
  /** Live tweaking : signale si le jeu tourne dans l'émulateur. */
  onRunningChange?: (running: boolean) => void;
}) {
  const [state, setState] = useState<PlayState>("idle");
  const [message, setMessage] = useState("");
  const [running, setRunning] = useState<boolean | null>(null);
  const [emulator, setEmulator] = useState(
    () => localStorage.getItem("psxstudio.emulator") ?? "pcsx-redux",
  );
  const pollRef = useRef<number>(0);

  useEffect(() => {
    localStorage.setItem("psxstudio.emulator", emulator);
  }, [emulator]);

  useEffect(() => {
    onRunningChange?.(state === "launched" && running === true);
  }, [state, running, onRunningChange]);

  /* Poll du statut quand l'émulateur est lancé. */
  useEffect(() => {
    if (state !== "launched") return;
    const tick = async () => {
      try {
        setRunning(await api.reduxStatus(PORT));
      } catch {
        setRunning(null); // API pas (encore) joignable
      }
    };
    tick();
    pollRef.current = window.setInterval(tick, 1500);
    return () => window.clearInterval(pollRef.current);
  }, [state]);

  const onPlay = async () => {
    if (!projectDir) return;
    setState("building");
    setMessage("build en cours…");
    try {
      const summary = await api.play(projectDir, emulator, PORT);
      setState("launched");
      setMessage(
        `${summary.converted} convertis, ${summary.cached} en cache — émulateur lancé`,
      );
    } catch (e) {
      setState("error");
      setMessage(String(e));
    }
  };

  return (
    <div className="playbar">
      <button
        className="button play"
        onClick={onPlay}
        disabled={state === "building" || !projectDir}
        title="Build incrémental + lancement PCSX-Redux"
      >
        ▶ Play
      </button>
      {!projectDir && (
        <span className="muted">ouvre un projet (project.json) pour builder et lancer</span>
      )}
      {state === "launched" && (
        <>
          <span className={`status-dot ${running === null ? "off" : running ? "run" : "pause"}`} />
          <span className="status-text">
            {running === null ? "API hors ligne" : running ? "en cours" : "en pause"}
          </span>
          <button
            className="button"
            disabled={running === null}
            onClick={() => (running ? api.reduxPause(PORT) : api.reduxResume(PORT))}
          >
            {running ? "⏸ Pause" : "⏵ Reprendre"}
          </button>
          <button
            className="button"
            disabled={running === null}
            onClick={() => {
              api.reduxReset(PORT);
              api.reduxClearBeacon().catch(() => {});
            }}
          >
            ↺ Reset
          </button>
        </>
      )}
      <input
        className="emulator-path"
        value={emulator}
        onChange={(e) => setEmulator(e.target.value)}
        title="Chemin de PCSX-Redux (doit être dans le PATH ou chemin complet)"
        spellCheck={false}
      />
      {message && <span className={state === "error" ? "error" : "muted"}>{message}</span>}
    </div>
  );
}
