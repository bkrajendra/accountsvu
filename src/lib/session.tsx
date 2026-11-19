import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, LOCKED_EVENT, type LockStatus } from "./api";
import { useToast } from "../components/Toast";
import { LockScreen } from "../components/LockScreen";

interface Session {
  lock: LockStatus;
  setLock: (s: LockStatus) => void;
  lockNow: () => Promise<void>;
}

const SessionContext = createContext<Session | null>(null);

const OFF: LockStatus = { enabled: false, locked: false, idleMinutes: 0 };
const ACTIVITY = ["pointerdown", "keydown", "wheel", "pointermove"] as const;

/**
 * App lock (spec D14). The desktop host refuses every command while locked;
 * this gate shows the lock screen instead of the app, drops cached data, and
 * locks again after the chosen idle time.
 */
export function SessionGate({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [lock, setLockState] = useState<LockStatus | null>(null);
  const lastActivity = useRef(Date.now());

  const setLock = useCallback(
    (s: LockStatus) => {
      // Nothing from the books stays in memory behind the lock screen.
      if (s.locked) qc.clear();
      setLockState(s);
    },
    [qc],
  );

  useEffect(() => {
    let live = true;
    api
      .appSession()
      .then((s) => {
        if (!live) return;
        setLock(s.lock);
        if (s.restoredKeptCopy) toast(`Backup restored. Your previous data was kept as ${s.restoredKeptCopy}.`);
      })
      .catch(() => live && setLockState(OFF));
    return () => {
      live = false;
    };
  }, [setLock, toast]);

  // Any command refused with "locked" (e.g. locked from elsewhere) shows the lock screen.
  useEffect(() => {
    const onLocked = () => setLock({ ...(lock ?? OFF), enabled: true, locked: true });
    window.addEventListener(LOCKED_EVENT, onLocked);
    return () => window.removeEventListener(LOCKED_EVENT, onLocked);
  }, [lock, setLock]);

  const lockNow = useCallback(async () => {
    setLock(await api.lockApp());
  }, [setLock]);

  // Idle lock: activity just stamps a time; a slow interval compares it.
  const idle = lock?.enabled && !lock.locked ? lock.idleMinutes : 0;
  useEffect(() => {
    if (!idle) return;
    lastActivity.current = Date.now();
    const stamp = () => {
      lastActivity.current = Date.now();
    };
    for (const e of ACTIVITY) window.addEventListener(e, stamp, { passive: true });
    const timer = window.setInterval(() => {
      if (Date.now() - lastActivity.current >= idle * 60_000) void lockNow();
    }, 10_000);
    return () => {
      for (const e of ACTIVITY) window.removeEventListener(e, stamp);
      window.clearInterval(timer);
    };
  }, [idle, lockNow]);

  if (!lock) return null;
  if (lock.locked) return <LockScreen onUnlocked={setLock} />;
  return <SessionContext.Provider value={{ lock, setLock, lockNow }}>{children}</SessionContext.Provider>;
}

export function useSession(): Session {
  const s = useContext(SessionContext);
  if (!s) throw new Error("useSession outside SessionGate");
  return s;
}
