import { applyDatabaseMutations } from './database-transaction';
import { recordStorageEvent } from '../diagnostics';
import { storageCatalog, type StoreId } from '../catalog';
import { ClientStorageError, type StorageChange, type StorageRecord } from '../types';

export type Mutation = { key: string; store: StoreId; raw: string | null; onlyIfAbsent?: boolean };
let opening: Promise<IDBDatabase> | undefined;
const factoryKey = Symbol.for('ghostex.client-storage.database-adapter');
const globals = globalThis as typeof globalThis & { [factoryKey]?: { open?: IDBFactory['open']; guarded: boolean } };
const factory = (globals[factoryKey] ??= { guarded: false });
function openNative(name: string, version?: number): IDBOpenDBRequest {
  factory.open ??= indexedDB.open.bind(indexedDB);
  return factory.open(name, version);
}
export function installDatabaseGuard(): void {
  if (factory.guarded) return;
  factory.guarded = true;
  factory.open ??= indexedDB.open.bind(indexedDB);
  const reject = (name: string): never => {
    recordStorageEvent({ store: name, operation: 'unexpected', bytes: 0, reason: 'unregistered' });
    throw new ClientStorageError(
      'unregistered',
      name,
      'Direct database access is forbidden. Use a registered client storage handle.'
    );
  };
  IDBFactory.prototype.open = function (name) {
    return reject(name);
  };
  IDBFactory.prototype.deleteDatabase = function (name) {
    return reject(name);
  };
}
const DATABASE = 'ghostex-client-storage';
function request<T>(value: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    value.onsuccess = () => resolve(value.result);
    value.onerror = () => reject(value.error);
  });
}
export function openDatabase(): Promise<IDBDatabase> {
  if (!opening) {
    opening = new Promise<IDBDatabase>((resolve, reject) => {
      const value = openNative(DATABASE, 1);
      value.onupgradeneeded = () => {
        value.result.createObjectStore('records', { keyPath: 'key' }).createIndex('store', 'store');
        value.result.createObjectStore('metadata');
      };
      value.onsuccess = () => {
        const db = value.result;
        db.onversionchange = () => {
          db.close();
          opening = undefined;
        };
        resolve(db);
      };
      value.onerror = () => reject(value.error);
      value.onblocked = () =>
        reject(new ClientStorageError('unavailable', '', 'Close older Ghostex pages to finish upgrading storage.'));
    }).catch((error: unknown) => {
      opening = undefined;
      throw error;
    });
  }
  return opening;
}
export async function readDatabaseSnapshot(): Promise<{ rows: StorageRecord[]; revision: number }> {
  const db = await openDatabase();
  const transaction = db.transaction(['records', 'metadata']);
  const [rows, revision] = await Promise.all([
    request<StorageRecord[]>(transaction.objectStore('records').getAll()),
    request(transaction.objectStore('metadata').get('revision')),
  ]);
  return { rows, revision: revision ?? 0 };
}
export async function readDatabase(): Promise<StorageRecord[]> {
  return (await readDatabaseSnapshot()).rows;
}

/** All pages share this strict transaction and its usage counters. No key enumeration on ordinary protected writes. */
export async function commitDatabase(
  mutations: readonly Mutation[],
  migration = false,
  importSource?: string
): Promise<StorageChange[]> {
  return transactionDatabase(() => mutations, [], migration, importSource);
}
export async function transactionDatabase(
  action: (rows: StorageRecord[]) => readonly Mutation[],
  ids: readonly StoreId[],
  migration = false,
  importSource?: string
): Promise<StorageChange[]> {
  const db = await openDatabase();
  return new Promise((resolve, reject) => {
    const transaction = db.transaction(['records', 'metadata'], 'readwrite', { durability: 'strict' });
    let result: StorageChange[] = [];
    let failure: unknown;
    transaction.oncomplete = () => resolve(result);
    transaction.onabort = () =>
      reject(failure ?? transaction.error ?? new Error('Storage transaction was interrupted.'));
    transaction.onerror = () => {
      failure ??= transaction.error;
    };
    void (async () => {
      const receiptKey = importSource ? storageCatalog.migrationReceipts.key + importSource : undefined;
      if (receiptKey && (await request(transaction.objectStore('records').get(receiptKey)))) return;
      const rows: StorageRecord[] = [];
      for (const id of ids)
        rows.push(
          ...((await request(transaction.objectStore('records').index('store').getAll(id))) as StorageRecord[])
        );
      const mutations = [...action(rows)];
      if (receiptKey) mutations.push({ key: receiptKey, store: 'migrationReceipts', raw: '1' });
      const table = transaction.objectStore('records');
      const metadata = transaction.objectStore('metadata');
      result = await applyDatabaseMutations({
        get: (key) => request(table.get(key)),
        byStore: (store) => request(table.index('store').getAll(store)),
        put: (row) => request(table.put(row)),
        delete: (key) => request(table.delete(key)),
      }, {
        get: (key) => request(metadata.get(key)),
        put: (value, key) => request(metadata.put(value, key)),
      }, mutations, migration);
    })().catch((error: unknown) => {
      failure = error;
      transaction.abort();
    });
  });
}

/** Open a legacy database without creating an empty database on fresh installs. */
export async function readLegacyDatabase(
  name: string,
  store: string
): Promise<{ rows: unknown[]; close: () => void; retire: (indices?: readonly number[]) => Promise<void> } | undefined> {
  const db = await new Promise<IDBDatabase | undefined>((resolve, reject) => {
    const value = openNative(name);
    let absent = false;
    value.onupgradeneeded = () => {
      absent = true;
      value.transaction!.abort();
    };
    value.onsuccess = () => resolve(value.result);
    value.onerror = () => (absent ? resolve(undefined) : reject(value.error));
    value.onblocked = () => reject(new Error(`Legacy storage ${name} is blocked.`));
  });
  if (!db) return undefined;
  if (!db.objectStoreNames.contains(store)) {
    db.close();
    return undefined;
  }
  const transaction = db.transaction(store);
  const table = transaction.objectStore(store);
  const [rows, keys] = await Promise.all([request(table.getAll()), request(table.getAllKeys())]);
  return {
    rows,
    close: () => db.close(),
    retire: (indices = rows.map((_, index) => index)) =>
      new Promise<void>((resolve, reject) => {
        const removal = db.transaction(store, 'readwrite', { durability: 'strict' });
        const records = removal.objectStore(store);
        for (const index of indices) {
          const current = records.get(keys[index]!);
          current.onsuccess = () => {
            if (JSON.stringify(current.result) === JSON.stringify(rows[index])) records.delete(keys[index]!);
          };
        }
        removal.oncomplete = () => {
          db.close();
          resolve();
        };
        removal.onabort = () => {
          db.close();
          reject(removal.error);
        };
        removal.onerror = () => {
          db.close();
          reject(removal.error);
        };
      }),
  };
}
