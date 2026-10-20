import { definitions, definitionForKey, owns, storageCatalog, type StoreId } from './catalog';
import { admission, storageBytes, STORAGE_BUDGETS } from './budgets';
import {
  notifyStorage,
  recordStorageEvent,
  storageEvents,
  subscribeStorage,
  writesInLastMinute,
  lastStorageFailure,
} from './diagnostics';
import {
  installBrowserGuard as guardBrowserStorage,
  readBrowser,
  scanBrowser,
  subscribeBrowser,
  writeBrowser,
  type BrowserBackend,
} from './adapters/browser';
import {
  commitDatabase,
  readDatabaseSnapshot,
  transactionDatabase,
  type Mutation,
} from './adapters/database';
import { migrateStorage, upgradeStorageSchemas, legacyStorageIssues } from './migration';
import {
  connectStoragePeers,
  publishStorageStatus,
  requestStoragePeerStatus,
  peerStorageUsage,
  retryStoragePeers,
  clearStoragePeerCache,
} from './peers';
import {
  ClientStorageError,
  type StorageChange,
  type StorageInspection,
  type StorageRecord,
  type StoreDefinition,
} from './types';

const rows = new Map<string, StorageRecord>();
const durableRows = new Map<string, StorageRecord>();
const pending = new Map<string, Mutation>();
const revisions = new Map<string, number>();
let snapshotRevision = 0;
const failures = new Map<string, ClientStorageError>();
const changes = new Set<(event: StorageChange) => void>();
let initialized: Promise<void> | undefined;
let ready = false;
let browserLoaded = false;
let channel: BroadcastChannel | undefined;
let tail: Promise<void> = Promise.resolve();
let scheduled = false;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
let retryDelay = 1_000;
let atomicView: Map<string, StorageRecord> | undefined;
let atomicMutations: Map<string, Mutation> | undefined;

function failure(definition: StoreDefinition, error: unknown): ClientStorageError {
  const result =
    error instanceof ClientStorageError
      ? error
      : new ClientStorageError(
          error instanceof DOMException && error.name === 'QuotaExceededError' ? 'quota' : 'unavailable',
          definition.id,
          error instanceof Error ? error.message : String(error)
        );
  failures.set(definition.id, result);
  recordStorageEvent({ store: definition.id, operation: 'failure', bytes: 0, reason: result.reason });
  return result;
}
function row(key: string, raw: string, definition: StoreDefinition, revision = 0): StorageRecord {
  const retainedAt = definition.retainedAt?.(raw);
  return {
    key,
    raw,
    store: definition.id,
    bytes: storageBytes(key, raw),
    updatedAt: retainedAt !== undefined && Number.isFinite(retainedAt) ? retainedAt : Date.now(),
    revision,
    schemaVersion: definition.version,
  };
}
function emit(event: StorageChange): void {
  for (const callback of changes) {
    try {
      callback(event);
    } catch (error) {
      console.error('[client-storage] Subscriber failed.', error);
    }
  }
  notifyStorage();
}
function accept(event: StorageChange): void {
  const definition = storageCatalog[event.store as StoreId];
  if (!definition || !owns(definition, event.key)) return;
  if (event.revision > 0 && event.revision <= (revisions.get(event.key) ?? snapshotRevision)) return;
  revisions.set(event.key, event.revision);
  if (event.raw === null) durableRows.delete(event.key);
  else durableRows.set(event.key, row(event.key, event.raw, definition, event.revision));
  if (pending.has(event.key)) {
    notifyStorage();
    return;
  }
  if (event.raw === null) rows.delete(event.key);
  else rows.set(event.key, row(event.key, event.raw, definition, event.revision));
  emit(event);
}
function loadBrowser(): void {
  if (browserLoaded || typeof window === 'undefined') return;
  browserLoaded = true;
  for (const backend of ['local', 'session'] as const) {
    try {
      for (const [key, raw] of scanBrowser(backend)) {
        const definition = definitionForKey(key);
        if (definition?.backend === backend) {
          const entry = row(key, raw, definition);
          rows.set(key, entry);
          durableRows.set(key, entry);
        }
      }
    } catch (error) {
      for (const definition of definitions.filter((entry) => entry.backend === backend)) failure(definition, error);
    }
  }
  subscribeBrowser((backend, key) => {
    if (key === null) {
      for (const [name, value] of rows)
        if (storageCatalog[value.store as StoreId]?.backend === backend) {
          rows.delete(name);
          durableRows.delete(name);
          emit({ key: name, store: value.store, raw: null, revision: 0 });
        }
      return;
    }
    const definition = definitionForKey(key);
    if (definition?.backend !== backend) return;
    const raw = readBrowser(backend, key);
    if (raw === null) {
      rows.delete(key);
      durableRows.delete(key);
    } else {
      const entry = row(key, raw, definition);
      rows.set(key, entry);
      durableRows.set(key, entry);
    }
    emit({ key, store: definition.id, raw, revision: 0 });
  });
}

/** Direct Storage writes reach here only from the development guard; keys of a registered external store take the ordinary metered path. */
function forwardExternalWrite(backend: BrowserBackend, key: string, raw: string | null): boolean {
  const definition = definitionForKey(key);
  if (!definition?.external || definition.backend !== backend) return false;
  writeManaged(definition, key, raw);
  return true;
}
export function installBrowserGuard(): void {
  guardBrowserStorage(forwardExternalWrite);
}

export function initializeClientStorage(): Promise<void> {
  if (initialized) return initialized;
  if (process.env.NODE_ENV !== 'production') installBrowserGuard();
  loadBrowser();
  initialized = (async () => {
    await migrateStorage();
    await upgradeStorageSchemas();
    const snapshot = await readDatabaseSnapshot();
    for (const entry of snapshot.rows) {
      rows.set(entry.key, entry);
      durableRows.set(entry.key, entry);
    }
    snapshotRevision = snapshot.revision;
    ready = true;
    if (typeof BroadcastChannel !== 'undefined') {
      channel = new BroadcastChannel('ghostex-client-storage-v1');
      channel.onmessage = (message: MessageEvent<StorageChange[]>) => {
        if (Array.isArray(message.data)) for (const event of message.data) accept(event);
      };
      connectStoragePeers(channel, pendingUsage, retryLocalStorage, clearLocalCache);
    }
    const resync = () => {
      void readDatabaseSnapshot()
        .then((snapshot) => {
          const names = new Set(snapshot.rows.map((entry) => entry.key));
          for (const [key, entry] of rows)
            if (storageCatalog[entry.store as StoreId]?.backend === 'indexeddb' && !names.has(key))
              accept({ key, store: entry.store, raw: null, revision: snapshot.revision });
          for (const entry of snapshot.rows) accept({ ...entry, revision: snapshot.revision });
          snapshotRevision = Math.max(snapshotRevision, snapshot.revision);
          for (const [key, revision] of revisions) if (revision <= snapshotRevision) revisions.delete(key);
        })
        .catch((error: unknown) => failure(storageCatalog.drafts, error));
    };
    window.addEventListener('focus', resync);
    window.addEventListener('pageshow', (event) => {
      if (event.persisted) resync();
    });
    resync();
    window.addEventListener('online', retryLocalStorage);
    window.addEventListener('pagehide', () => {
      void flushClientStorage().catch(() => {});
    });
    notifyStorage();
  })().catch((error: unknown) => {
    initialized = undefined;
    failure(storageCatalog.drafts, error);
    throw error;
  });
  return initialized;
}

function ensure(definition: StoreDefinition): void {
  loadBrowser();
  if (definition.backend === 'indexeddb' && !ready)
    throw new ClientStorageError('unavailable', definition.id, 'Storage must finish loading before this view opens.');
}
function validate(definition: StoreDefinition, key: string): void {
  if (!owns(definition, key))
    throw new ClientStorageError('unregistered', definition.id, `${definition.owner} does not own ${key}.`);
}
export function readManaged(definition: StoreDefinition, key: string): string | null {
  validate(definition, key);
  ensure(definition);
  if (atomicView && definition.backend === 'indexeddb') return atomicView.get(key)?.raw ?? null;
  if (definition.backend !== 'indexeddb') {
    try {
      return readBrowser(definition.backend, key);
    } catch (error) {
      throw failure(definition, error);
    }
  }
  const entry = rows.get(key);
  if (
    entry &&
    definition.policy === 'cache' &&
    definition.maxAgeMs !== null &&
    Date.now() - entry.updatedAt > definition.maxAgeMs
  ) {
    writeManaged(definition, key, null);
    return null;
  }
  return entry?.raw ?? null;
}
export function managedKeys(ids: readonly StoreId[]): string[] {
  for (const id of ids) ensure(storageCatalog[id]);
  return [...(atomicView ?? rows).values()]
    .filter((entry) => ids.includes(entry.store as StoreId))
    .map((entry) => entry.key);
}
export function writeManaged(definition: StoreDefinition, key: string, raw: string | null): void {
  validate(definition, key);
  ensure(definition);
  try {
    if (raw !== null) {
      try {
        definition.codec.decode(raw);
      } catch {
        throw new ClientStorageError(
          'schema',
          definition.id,
          `${definition.owner} rejected an invalid ${definition.codec.schema} value.`
        );
      }
    }
    if (
      (definition.backend === 'indexeddb'
        ? ((atomicView ?? rows).get(key)?.raw ?? null)
        : readBrowser(definition.backend, key)) === raw
    )
      return;
    const mutation: Mutation = { store: definition.id as StoreId, key, raw };
    if (atomicView && atomicMutations) {
      if (definition.backend !== 'indexeddb') throw new Error('Atomic updates require an IndexedDB store.');
      atomicMutations.set(key, mutation);
      if (raw === null) atomicView.delete(key);
      else atomicView.set(key, row(key, raw, definition));
      return;
    }
    if (definition.backend !== 'indexeddb') {
      // Preferences are small and infrequently written. Refresh the native area here so another page's queued event cannot hide its usage.
      const nativeRows = scanBrowser(definition.backend).map(([name, value]) => ({
        key: name,
        raw: value,
        bytes: storageBytes(name, value),
        store: definitionForKey(name)?.id ?? '',
        updatedAt: rows.get(name)?.updatedAt ?? Date.now(),
        revision: 0,
        schemaVersion: definitionForKey(name)?.version ?? 0,
      }));
      if (raw !== null) {
        const removals = admission(
          definition,
          row(key, raw, definition),
          nativeRows.filter((entry) => entry.store === definition.id),
          nativeRows.reduce((sum, entry) => sum + entry.bytes, 0)
        );
        // Store the replacement first. A browser quota failure must leave the old cache intact.
        writeBrowser(definition.backend, key, raw);
        for (const name of removals) {
          writeBrowser(definition.backend, name, null);
          rows.delete(name);
          durableRows.delete(name);
        }
        if (removals.includes(key)) raw = null;
      } else writeBrowser(definition.backend, key, null);
      if (raw === null) durableRows.delete(key);
      else durableRows.set(key, row(key, raw, definition));
      failures.delete(definition.id);
      recordStorageEvent({
        store: definition.id,
        operation: raw === null ? 'remove' : 'write',
        bytes: raw === null ? 0 : storageBytes(key, raw),
      });
    } else {
      pending.set(key, mutation);
      publishStorageStatus();
    }
    if (raw === null) rows.delete(key);
    else rows.set(key, row(key, raw, definition));
    emit({ key, raw, store: definition.id, revision: 0 });
    if (definition.backend === 'indexeddb') schedule();
  } catch (error) {
    throw failure(definition, error);
  }
}

function schedule(): void {
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(() => {
    scheduled = false;
    void flushClientStorage().catch(() => {});
  });
}
async function drain(): Promise<void> {
  let firstError: unknown;
  const failedStores = new Set<StoreId>();
  while ([...pending.values()].some((entry) => !failedStores.has(entry.store))) {
    const ids = [...new Set([...pending.values()].map((entry) => entry.store))].filter((id) => !failedStores.has(id));
    // A full disposable cache must not hold unrelated unsent work hostage.
    for (const id of ids.sort(
      (a, b) => Number(storageCatalog[b].policy === 'protected') - Number(storageCatalog[a].policy === 'protected')
    )) {
      const batch = [...pending.values()].filter((entry) => entry.store === id);
      try {
        const committed = await commitDatabase(batch);
        for (const mutation of batch) if (pending.get(mutation.key) === mutation) pending.delete(mutation.key);
        failures.delete(id);
        for (const event of committed) {
          accept(event);
          recordStorageEvent({
            store: event.store,
            operation: event.raw === null ? 'remove' : 'write',
            bytes: event.raw === null ? 0 : storageBytes(event.key, event.raw),
          });
        }
        channel?.postMessage(committed);
      } catch (error) {
        firstError ??= error;
        failedStores.add(id);
        failure(storageCatalog[id], error);
      }
    }
  }
  notifyStorage();
  publishStorageStatus();
  if (firstError) {
    if (!retryTimer)
      retryTimer = setTimeout(() => {
        retryTimer = undefined;
        retryLocalStorage();
      }, retryDelay);
    retryDelay = Math.min(30_000, retryDelay * 2);
    throw firstError;
  }
  retryDelay = 1_000;
}
/** Resolve only when every pending mutation is committed, including edits queued during the wait. */
export function flushClientStorage(ids?: readonly StoreId[]): Promise<void> {
  tail = tail.catch(() => {}).then(drain);
  return tail.catch((error: unknown) => {
    if (!ids || ids.some((id) => failures.has(id))) throw error;
  });
}
function retryLocalStorage(): void {
  if (retryTimer) clearTimeout(retryTimer);
  retryTimer = undefined;
  void flushClientStorage().catch(() => {});
}
export function retryClientStorage(): void {
  retryStoragePeers();
  retryLocalStorage();
}
export function isStoragePending(id: StoreId, key?: string): boolean {
  return failures.has(id) || (key ? pending.has(key) : [...pending.values()].some((entry) => entry.store === id));
}
export function storageFailure(id: StoreId): ClientStorageError | undefined {
  return failures.get(id);
}
export function subscribeManaged(ids: readonly StoreId[], callback: (event: StorageChange) => void): () => void {
  const filtered = (event: StorageChange) => {
    if (ids.includes(event.store as StoreId)) callback(event);
  };
  changes.add(filtered);
  return () => {
    changes.delete(filtered);
  };
}

/** The callback is synchronous and reads the latest committed values under the origin-wide transaction. */
export async function atomicStorage(ids: readonly StoreId[], action: () => void): Promise<void> {
  await initializeClientStorage();
  await flushClientStorage(ids);
  const committed = await transactionDatabase((latest) => {
    atomicView = new Map(latest.map((entry) => [entry.key, entry]));
    atomicMutations = new Map();
    try {
      action();
      return [...atomicMutations.values()];
    } finally {
      atomicView = undefined;
      atomicMutations = undefined;
    }
  }, ids).catch((error: unknown) => {
    throw failure(storageCatalog[ids[0] ?? 'drafts'], error);
  });
  for (const event of committed) {
    accept(event);
    recordStorageEvent({
      store: event.store,
      operation: event.raw === null ? 'remove' : 'write',
      bytes: event.raw === null ? 0 : storageBytes(event.key, event.raw),
    });
  }
  channel?.postMessage(committed);
}

function pendingUsage() {
  const usage = new Map<StoreId, { store: StoreId; count: number; bytes: number }>();
  for (const entry of pending.values()) {
    const total = usage.get(entry.store) ?? { store: entry.store, count: 0, bytes: 0 };
    total.count++;
    total.bytes += entry.raw === null ? 0 : storageBytes(entry.key, entry.raw);
    usage.set(entry.store, total);
  }
  return [...usage.values()];
}

export function inspectClientStorage(): StorageInspection {
  loadBrowser();
  requestStoragePeerStatus();
  const events = storageEvents();
  const unknown: StorageInspection['unknown'] = legacyStorageIssues();
  const totals: StorageInspection['totals'] = {
    local: { bytes: 0, budget: STORAGE_BUDGETS.local },
    session: { bytes: 0, budget: STORAGE_BUDGETS.session },
    indexeddb: { bytes: 0, budget: STORAGE_BUDGETS.indexeddb },
  };
  totals.indexeddb.bytes = unknown.reduce((sum, entry) => sum + entry.bytes, 0);
  for (const backend of ['local', 'session'] as const) {
    try {
      const current = scanBrowser(backend);
      const names = new Set(current.map(([key]) => key));
      for (const [key, entry] of durableRows)
        if (storageCatalog[entry.store as StoreId]?.backend === backend && !names.has(key)) durableRows.delete(key);
      for (const [key, raw] of current) {
        const bytes = storageBytes(key, raw);
        totals[backend].bytes += bytes;
        const definition = definitionForKey(key);
        if (definition?.backend !== backend) unknown.push({ backend, key, bytes });
        else durableRows.set(key, row(key, raw, definition));
      }
    } catch {
      /* Availability is already reported against the registered stores. */
    }
  }
  for (const entry of durableRows.values())
    if (!storageCatalog[entry.store as StoreId]) {
      unknown.push({ backend: 'indexeddb', key: entry.key, bytes: entry.bytes });
      totals.indexeddb.bytes += entry.bytes;
    }
  const stores = definitions.map((definition) => {
    const peers = peerStorageUsage(definition.id);
    const entries = [...durableRows.values()].filter((entry) => entry.store === definition.id);
    const bytes = entries.reduce((sum, entry) => sum + entry.bytes, 0);
    if (definition.backend === 'indexeddb') totals.indexeddb.bytes += bytes;
    return {
      ...definition,
      schema: `${definition.codec.schema} v${definition.version}`,
      bytes,
      entries: entries.length,
      pending: peers.pending + [...pending.values()].filter((entry) => entry.store === definition.id).length,
      pendingBytes:
        peers.pendingBytes +
        [...pending.values()]
          .filter((entry) => entry.store === definition.id)
          .reduce((sum, entry) => sum + (entry.raw === null ? 0 : storageBytes(entry.key, entry.raw)), 0),
      writesPerMinute: peers.writesPerMinute + writesInLastMinute(definition.id),
      lastFailure: lastStorageFailure(definition.id),
      largest: entries
        .sort((a, b) => b.bytes - a.bytes)
        .slice(0, 5)
        .map(({ key, bytes, updatedAt }) => ({ key, bytes, updatedAt })),
    };
  });
  return { measuredAt: Date.now(), ready, stores, unknown, totals, events };
}
export async function clearDisposableStorage(id: StoreId): Promise<void> {
  const definition = storageCatalog[id];
  if (definition?.policy !== 'cache') throw new Error('Only disposable caches can be cleared here.');
  clearStoragePeerCache(id);
  await clearLocalCache(id);
}
async function clearLocalCache(id: StoreId): Promise<void> {
  const definition = storageCatalog[id];
  if (definition.backend !== 'indexeddb') {
    for (const [key] of scanBrowser(definition.backend)) if (owns(definition, key)) writeManaged(definition, key, null);
    return;
  }
  for (const [key, mutation] of pending) {
    if (mutation.store !== id) continue;
    pending.delete(key);
    const durable = durableRows.get(key);
    if (durable) rows.set(key, durable);
    else rows.delete(key);
    emit({ key, store: id, raw: durable?.raw ?? null, revision: 0 });
  }
  failures.delete(id);
  publishStorageStatus();
  await tail.catch(() => {});
  failures.delete(id);
  await atomicStorage([id], () => {
    for (const key of managedKeys([id])) writeManaged(definition, key, null);
  });
}
export { subscribeStorage };
