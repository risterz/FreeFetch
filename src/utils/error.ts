/**
 * Safely formats any error of unknown type into a user-friendly, informative string.
 * Guarantees that `[object Object]` is NEVER returned under any circumstances.
 */
export function formatErrorMessage(err: unknown): string {
  if (err === null || err === undefined) {
    return 'Unknown error';
  }

  if (typeof err === 'string') {
    const trimmed = err.trim();
    return trimmed.length > 0 ? trimmed : 'Unknown error';
  }

  if (typeof err === 'number' || typeof err === 'boolean') {
    return String(err);
  }

  if (err instanceof Error) {
    if (err.message && err.message.trim().length > 0) {
      return err.message.trim();
    }
    return err.name || 'Error';
  }

  if (typeof err === 'object') {
    const obj = err as Record<string, unknown>;

    // Case 1: Nested Tauri IPC error where message is an object:
    // e.g. { type: "Extractor", message: { service: "youtube", message: "..." } }
    if (obj.message && typeof obj.message === 'object') {
      const nested = obj.message as Record<string, unknown>;
      if (typeof nested.message === 'string' && nested.message.trim().length > 0) {
        if (typeof nested.service === 'string' && nested.service.trim().length > 0) {
          return `[${nested.service}] ${nested.message.trim()}`;
        }
        return nested.message.trim();
      }
      if (typeof nested.error === 'string' && nested.error.trim().length > 0) {
        return nested.error.trim();
      }
      if (typeof nested.details === 'string' && nested.details.trim().length > 0) {
        return nested.details.trim();
      }
    }

    // Case 2: obj.message is a non-empty string
    if (typeof obj.message === 'string' && obj.message.trim().length > 0) {
      const prefix =
        typeof obj.service === 'string' && obj.service.trim().length > 0
          ? `[${obj.service.trim()}] `
          : '';
      return `${prefix}${obj.message.trim()}`;
    }

    // Case 3: obj.details or obj.error is a non-empty string
    if (typeof obj.details === 'string' && obj.details.trim().length > 0) {
      return obj.details.trim();
    }
    if (typeof obj.error === 'string' && obj.error.trim().length > 0) {
      return obj.error.trim();
    }

    // Case 4: obj.type exists (e.g. { type: "Cancelled" } or { type: "FfmpegNotFound" })
    if (typeof obj.type === 'string' && obj.type.trim().length > 0) {
      return obj.type.trim();
    }

    // Case 5: obj.kind or obj.code
    if (typeof obj.kind === 'string' && obj.kind.trim().length > 0) {
      return obj.kind.trim();
    }
    if (typeof obj.code === 'number' || typeof obj.code === 'string') {
      return `Error code: ${obj.code}`;
    }

    // Case 6: Fallback to JSON serialization (never produces [object Object])
    try {
      const json = JSON.stringify(err);
      if (json && json !== '{}') {
        return json;
      }
    } catch {
      // Ignore cyclic structure serialization errors
    }
  }

  const str = String(err);
  return str === '[object Object]' ? 'Unknown structured error' : str;
}
