import { useEffect, useState } from 'react';
import type { IpcError } from '../types/catalog';

export function userMessage(error: unknown): string {
  if (typeof error === 'object' && error && 'message' in error) {
    const messages: Partial<Record<IpcError['code'], string>> = {
      SOURCE_OVERLAP: 'Cartella già registrata o sovrapposta a una sorgente esistente.',
      INVALID_PATH: 'Percorso non valido o non leggibile.',
      SOURCE_NOT_FOUND: 'Sorgente non trovata.',
      SOURCE_UNAVAILABLE: 'Sorgente non disponibile: controlla il disco o il percorso.',
      SCAN_ALREADY_RUNNING: 'Scansione già in corso per questa sorgente.',
      MODEL_NOT_FOUND: 'Il modello non è più nel catalogo.',
    };
    if ('code' in error && messages[error.code as IpcError['code']]) return messages[error.code as IpcError['code']]!;
    if (error instanceof TypeError) return 'Catalogo non raggiungibile. Apri MeshCatalog nell’app desktop e riprova.';
    return String(error.message);
  }
  return 'Operazione non riuscita. Riprova.';
}

// A new scope never displays the previous scope. A background refresh keeps the current view in place.
export function useCatalogRead<T>(load: () => Promise<T>, revision: number) {
  const [snapshot, setSnapshot] = useState<{ load: typeof load; value?: T; error?: string }>();
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let current = true;
    load().then(value => { if (current) setSnapshot({ load, value }); }, error => {
      if (current) setSnapshot(previous => ({ load, value: previous?.load === load ? previous.value : undefined, error: userMessage(error) }));
    });
    return () => { current = false; };
  }, [load, revision, attempt]);
  return { ...(snapshot?.load === load ? snapshot : {}), retry: () => setAttempt(n => n + 1) };
}
