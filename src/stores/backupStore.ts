import { create } from "zustand";
import * as api from "../lib/tauri/backups";
import type { BackupManifest, BackupScope } from "../lib/tauri/backups";

type BackupStore = {
  backups: BackupManifest[];
  loading: boolean;
  error: string | null;
  exportedPath: string | null;
  load: () => Promise<void>;
  checkpoint: (scope: BackupScope) => Promise<void>;
  restore: (id: string) => Promise<void>;
  exportZip: () => Promise<void>;
  importZip: (path: string) => Promise<void>;
};

const message = (error: unknown) => typeof error === "object" && error && "message" in error ? String(error.message) : String(error);

export const useBackupStore = create<BackupStore>((set, get) => ({
  backups: [], loading: false, error: null, exportedPath: null,
  load: async () => { set({ loading: true }); try { set({ backups: await api.listBackups(), error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  checkpoint: async (scope) => { set({ loading: true }); try { await api.createCheckpoint(scope); await get().load(); } catch (error) { set({ error: message(error), loading: false }); } },
  restore: async (id) => { set({ loading: true }); try { await api.restoreBackup(id); await get().load(); } catch (error) { set({ error: message(error), loading: false }); } },
  exportZip: async () => { set({ loading: true }); try { set({ exportedPath: await api.exportBackupZip(), error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  importZip: async (path) => { set({ loading: true }); try { await api.importBackupZip(path); await get().load(); } catch (error) { set({ error: message(error), loading: false }); } },
}));
