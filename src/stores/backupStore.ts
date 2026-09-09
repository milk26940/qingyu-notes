import { create } from "zustand";
import * as api from "../lib/tauri/backups";
import type { BackupManifest, BackupScope } from "../lib/tauri/backups";

type BackupStore = {
  backups: BackupManifest[];
  backupLocation: string | null;
  loading: boolean;
  error: string | null;
  exportedPath: string | null;
  importReport: api.ImportReport | null;
  importPreview: api.ImportPreview | null;
  previewPath: string | null;
  load: () => Promise<void>;
  checkpoint: (scope: BackupScope) => Promise<void>;
  restore: (id: string) => Promise<void>;
  exportZip: () => Promise<void>;
  importZip: (path: string) => Promise<void>;
  exportLossless: () => Promise<void>;
  exportMarkdown: () => Promise<void>;
  previewContent: (path: string) => Promise<void>;
  importContent: (path: string) => Promise<void>;
};

const message = (error: unknown) => typeof error === "object" && error && "message" in error ? String(error.message) : String(error);

export const useBackupStore = create<BackupStore>((set, get) => ({
  backups: [], backupLocation: null, loading: false, error: null, exportedPath: null, importReport: null, importPreview: null, previewPath: null,
  load: async () => {
    set({ loading: true });
    try {
      const [backups, backupLocation] = await Promise.all([
        api.listBackups(),
        api.backupLocation().catch(() => null),
      ]);
      set({ backups, backupLocation, error: null });
    } catch (error) {
      set({ error: message(error) });
    } finally {
      set({ loading: false });
    }
  },
  checkpoint: async (scope) => { set({ loading: true }); try { await api.createCheckpoint(scope); await get().load(); } catch (error) { set({ error: message(error), loading: false }); } },
  restore: async (id) => { set({ loading: true }); try { await api.restoreBackup(id); await get().load(); } catch (error) { set({ error: message(error), loading: false }); } },
  exportZip: async () => { set({ loading: true }); try { set({ exportedPath: await api.exportBackupZip(), error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  importZip: async (path) => { set({ loading: true }); try { await api.importBackupZip(path); await get().load(); } catch (error) { set({ error: message(error), loading: false }); } },
  exportLossless: async () => { set({ loading: true }); try { set({ exportedPath: await api.exportLosslessPackage(), error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  exportMarkdown: async () => { set({ loading: true }); try { set({ exportedPath: await api.exportMarkdownZip(), error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  previewContent: async (path) => { set({ loading: true, importPreview: null, previewPath: null, importReport: null }); try { set({ importPreview: await api.previewContent(path), previewPath: path, error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ loading: false }); } },
  importContent: async (path) => { set({ loading: true }); try { set({ importReport: await api.importContent(path), importPreview: null, previewPath: null, error: null }); await get().load(); } catch (error) { set({ error: message(error), loading: false }); } },
}));
