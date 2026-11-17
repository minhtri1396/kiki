// How the web UI reaches the Rust core. Pattern: Strategy, chosen once at load.
//
// * Inside the Tauri window (`window.__TAURI__` exists) requests go through the `api_request` IPC command.
//   The reply arrives as events on a Channel (`head`, `chunk`, `end`) and is turned back into a standard
//   `Response`, so callers keep using `res.json()` and `res.body.getReader()` (NDJSON generation streams).
// * In a browser (`kiki --no-open`) requests use plain `fetch` against the loopback server.
(function () {
  const tauri = window.__TAURI__ && window.__TAURI__.core;

  function abortError() {
    const error = new Error("The operation was aborted.");
    error.name = "AbortError";
    return error;
  }

  function decode(base64) {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }

  function hasNoBody(status) {
    return status === 204 || status === 205 || status === 304;
  }

  function ipcFetch(url, options = {}) {
    const method = (options.method || "GET").toUpperCase();
    const body = typeof options.body === "string" ? options.body : null;
    const signal = options.signal;
    return new Promise((resolve, reject) => {
      if (signal && signal.aborted) {
        reject(abortError());
        return;
      }
      let controller = null;
      let answered = false;
      const stream = new ReadableStream({
        start(c) {
          controller = c;
        },
      });
      const fail = (error) => {
        if (!answered) reject(error);
        else {
          try {
            controller.error(error);
          } catch (ignored) {}
        }
        answered = true;
      };
      if (signal) signal.addEventListener("abort", () => fail(abortError()), { once: true });

      const channel = new tauri.Channel();
      channel.onmessage = (message) => {
        if (message.event === "head") {
          const { status, headers } = message.data;
          answered = true;
          if (hasNoBody(status)) controller.close();
          resolve(new Response(hasNoBody(status) ? null : stream, { status, headers: new Headers(headers) }));
        } else if (message.event === "chunk") {
          try {
            controller.enqueue(decode(message.data));
          } catch (ignored) {}
        } else if (message.event === "end") {
          try {
            controller.close();
          } catch (ignored) {}
        }
      };
      tauri.invoke("api_request", { method, path: url, body, onEvent: channel }).catch((error) => fail(new Error(String(error))));
    });
  }

  window.kikiFetch = tauri ? ipcFetch : (url, options) => fetch(url, options);
})();
