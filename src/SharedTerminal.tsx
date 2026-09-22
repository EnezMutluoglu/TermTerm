import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { Users, RefreshCw } from "lucide-react";
import { Modal, Select, Busy } from "./components";
import { call, errorText, desktop } from "./api";
import type { Entity, Session, SyncProfile } from "./types";
export default function SharedTerminal({
  records,
  sessions,
  active,
  onJoin,
  onClose,
}: {
  records: Entity[];
  sessions: Session[];
  active: string;
  onJoin: (s: Session) => void;
  onClose: () => void;
}) {
  const profiles = records.filter((r) => r.kind === "syncProfile");
  const [profileId, setProfileId] = useState(profiles[0]?.id ?? "");
  const [localId, setLocalId] = useState(active);
  const [list, setList] = useState<any[]>([]);
  const [members, setMembers] = useState<any[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const profile = profiles.find((r) => r.id === profileId)?.data as
    SyncProfile | undefined;
  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function refresh() {
    if (!profile) return;
    setList(await call<any[]>("share_list", { profile }));
    setMembers(await call<any[]>("team_list", { profile }));
  }
  useEffect(() => {
    if (!desktop) return;
    void run(refresh);
    const un = listen<any>("share-event", ({ payload }) => {
      if (payload.ended) setList((l) => l.filter((s) => s.id !== payload.id));
      else
        setList((l) =>
          l.map((s) =>
            s.id === payload.id ? { ...s, writer: payload.writer } : s,
          ),
        );
    });
    return () => {
      void un.then((f) => f());
    };
  }, [profileId]);
  return (
    <Modal title="Shared terminals" onClose={onClose} wide>
      <div className="modal-body">
        <p>
          Share a live terminal with members of this PostgreSQL vault. The owner
          grants one editor control at a time. Sessions end when the owner
          disconnects.
        </p>
        <Select
          label="PostgreSQL profile"
          value={profileId}
          onChange={setProfileId}
          options={profiles.map((p) => ({ value: p.id, label: p.data.label }))}
        />
        {!profiles.length && (
          <div className="notice">
            Save a PostgreSQL profile and upload or open this vault before
            sharing.
          </div>
        )}
        <div className="settings-card">
          <h3>
            <Users size={17} />
            Share an open terminal
          </h3>
          <Select
            label="Local terminal"
            value={localId}
            onChange={setLocalId}
            options={[
              { value: "", label: "Select a connected terminal" },
              ...sessions
                .filter((s) => s.connected && !s.label.startsWith("Shared ·"))
                .map((s) => ({ value: s.id, label: s.label })),
            ]}
          />
          <button
            className="primary"
            disabled={busy || !localId || !profile}
            onClick={() =>
              void run(async () => {
                await call("share_start", { profile, localId, remoteId: null });
                await refresh();
              })
            }
          >
            Start sharing
          </button>
        </div>
        <div className="section-header">
          <h3>Live sessions</h3>
          <button
            className="secondary"
            disabled={busy || !profile}
            onClick={() => void run(refresh)}
          >
            <RefreshCw size={15} />
            Refresh
          </button>
        </div>
        {list.map((s) => (
          <div className="settings-card" key={s.id}>
            <p>
              <strong>{s.owner}</strong> · {s.id.slice(0, 8)}
            </p>
            <p className="muted">Writing: {s.writer}</p>
            {s.owner === profile?.username ? (
              <div className="button-row">
                <Select
                  label="Give keyboard control"
                  value={s.writer}
                  onChange={(writer) =>
                    void run(async () => {
                      await call("share_control", {
                        profile,
                        id: s.id,
                        writer,
                        finish: false,
                      });
                      await refresh();
                    })
                  }
                  options={members
                    .filter((m) => m.role !== "viewer")
                    .map((m) => ({ value: m.username, label: m.username }))}
                />
                <button
                  className="danger-button"
                  onClick={() =>
                    void run(async () => {
                      await call("share_control", {
                        profile,
                        id: s.id,
                        writer: profile?.username,
                        finish: true,
                      });
                      await refresh();
                    })
                  }
                >
                  End sharing
                </button>
              </div>
            ) : (
              <button
                className="primary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const result = await call<{ sessionId: string }>(
                      "share_start",
                      { profile, localId: null, remoteId: s.id },
                    );
                    onJoin({
                      id: result.sessionId,
                      label: "Shared · " + s.owner,
                      status: "Connected",
                      connected: true,
                      closed: false,
                    });
                    onClose();
                  })
                }
              >
                Join live terminal
              </button>
            )}
          </div>
        ))}
        {!list.length && <p className="muted">No live shared terminals.</p>}
        {busy && <Busy />}
        {error && <div className="notice error">{error}</div>}
        <p className="muted small">
          Only new output is streamed. Inputs expire after two seconds and are
          never replayed after reconnection. SSH and SFTP connections stay on
          the owner's computer.
        </p>
      </div>
      <footer>
        <button className="secondary" onClick={onClose}>
          Close
        </button>
      </footer>
    </Modal>
  );
}
