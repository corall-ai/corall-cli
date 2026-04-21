function abortError(message: string): Error {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}

export function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === "AbortError";
}

export async function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  if (ms <= 0) {
    return;
  }

  await new Promise<void>((resolve, reject) => {
    const cleanup = (): void => {
      clearTimeout(timeout);
      signal?.removeEventListener("abort", onAbort);
    };

    const onAbort = (): void => {
      cleanup();
      reject(abortError("Operation aborted"));
    };

    const timeout = setTimeout(() => {
      cleanup();
      resolve();
    }, ms);

    if (signal?.aborted) {
      cleanup();
      reject(abortError("Operation aborted"));
      return;
    }

    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

interface FetchWithTimeoutOptions extends RequestInit {
  timeoutMs: number;
  signal?: AbortSignal;
}

export async function fetchWithTimeout(
  url: URL,
  options: FetchWithTimeoutOptions,
): Promise<Response> {
  const { timeoutMs, signal, ...fetchOptions } = options;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  const onAbort = (): void => controller.abort();

  try {
    if (signal?.aborted) {
      throw abortError("Operation aborted");
    }

    signal?.addEventListener("abort", onAbort, { once: true });

    return await fetch(url, {
      ...fetchOptions,
      signal: controller.signal,
    });
  } catch (error: unknown) {
    if (controller.signal.aborted || signal?.aborted) {
      throw abortError(`Request aborted for ${url.toString()}`);
    }
    throw error;
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", onAbort);
  }
}

export async function fetchJson(url: URL, options: FetchWithTimeoutOptions): Promise<unknown> {
  const response = await fetchWithTimeout(url, options);
  const bodyText = await response.text();

  if (!response.ok) {
    const details = bodyText ? `: ${bodyText}` : "";
    throw new Error(`HTTP ${response.status} ${response.statusText}${details}`);
  }

  if (!bodyText) {
    return null;
  }

  return JSON.parse(bodyText) as unknown;
}

export async function fetchOk(url: URL, options: FetchWithTimeoutOptions): Promise<void> {
  const response = await fetchWithTimeout(url, options);
  const bodyText = await response.text();

  if (!response.ok) {
    const details = bodyText ? `: ${bodyText}` : "";
    throw new Error(`HTTP ${response.status} ${response.statusText}${details}`);
  }
}
