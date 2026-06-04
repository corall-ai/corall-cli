function abortError(message) {
    const error = new Error(message);
    error.name = "AbortError";
    return error;
}
export function isAbortError(error) {
    return error instanceof Error && error.name === "AbortError";
}
export async function sleep(ms, signal) {
    if (ms <= 0) {
        return;
    }
    await new Promise((resolve, reject) => {
        const cleanup = () => {
            clearTimeout(timeout);
            signal?.removeEventListener("abort", onAbort);
        };
        const onAbort = () => {
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
export async function fetchWithTimeout(url, options) {
    const { timeoutMs, signal, ...fetchOptions } = options;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), timeoutMs);
    const onAbort = () => controller.abort();
    try {
        if (signal?.aborted) {
            throw abortError("Operation aborted");
        }
        signal?.addEventListener("abort", onAbort, { once: true });
        return await fetch(url, {
            ...fetchOptions,
            signal: controller.signal,
        });
    }
    catch (error) {
        if (controller.signal.aborted || signal?.aborted) {
            throw abortError(`Request aborted for ${url.toString()}`);
        }
        throw error;
    }
    finally {
        clearTimeout(timeout);
        signal?.removeEventListener("abort", onAbort);
    }
}
export async function fetchJson(url, options) {
    const response = await fetchWithTimeout(url, options);
    const bodyText = await response.text();
    if (!response.ok) {
        const details = bodyText ? `: ${bodyText}` : "";
        throw new Error(`HTTP ${response.status} ${response.statusText}${details}`);
    }
    if (!bodyText) {
        return null;
    }
    return JSON.parse(bodyText);
}
export async function fetchOk(url, options) {
    const response = await fetchWithTimeout(url, options);
    const bodyText = await response.text();
    if (!response.ok) {
        const details = bodyText ? `: ${bodyText}` : "";
        throw new Error(`HTTP ${response.status} ${response.statusText}${details}`);
    }
}
//# sourceMappingURL=http.js.map