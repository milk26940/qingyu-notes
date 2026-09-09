import { invoke } from "@tauri-apps/api/core";

export type BackupScope = { kind: "vault" } | { kind: "page"; pageId: string };
export type BackupEntry = { path: string; hash: string; size: number };
export type BackupManifest = { id: string; createdAt: string; source: string; scope: BackupScope; entries: BackupEntry[] };

export const createCheckpoint = (scope: BackupScope, source = "manual") => invoke<BackupManifest>("backup_checkpoint", { scope, source });
export const listBackups = () => invoke<BackupManifest[]>("backup_list");
export const backupLocation = () => invoke<string>("backup_location");
export const restoreBackup = (snapshotId: string) => invoke<BackupManifest>("backup_restore", { snapshotId });
export const exportBackupZip = (path?: string) => invoke<string>("backup_export_zip", { path: path || null });
export const importBackupZip = (path: string) => invoke<BackupManifest>("backup_import_zip", { path });

export type ImportReport = { pages: number; files: number; warnings: string[] };
export type ImportPreview = { pages: number; databases: number; files: number; warnings: string[]; sourceKind: string };
export const exportLosslessPackage = (path?: string) => invoke<string>("transfer_export_lossless", { path: path || null });
export const exportMarkdownZip = (path?: string) => invoke<string>("transfer_export_markdown", { path: path || null });
export const previewContent = (path: string) => invoke<ImportPreview>("transfer_preview_content", { path });
export const importContent = (path: string) => invoke<ImportReport>("transfer_import_content", { path });
