// JS side of `rt_emscripten_jspi.rs`, linked with `--js-library` in the JSPI
// lane. `tokio_test_reenter` is a promising export (`-sJSPI_EXPORTS`), so
// calling it from a host timer starts a sibling activation.
addToLibrary({
  tokio_test_schedule_reenter: (ms) => {
    globalThis.tokioReenter = new Promise((resolve) =>
      setTimeout(() => resolve(wasmExports.tokio_test_reenter()), ms));
  },

  tokio_test_await_reenter__async: true,
  tokio_test_await_reenter__deps: ['$Asyncify'],
  tokio_test_await_reenter: () => Asyncify.handleAsync(() => globalThis.tokioReenter),

  // Plain JS frame between the promising activation and the export, so a
  // suspension inside it has no suspender and throws.
  tokio_test_call_unsuspendable: () => {
    try {
      wasmExports.tokio_test_unsuspendable();
      return 0;
    } catch (e) {
      return e instanceof WebAssembly.SuspendError ? 1 : 2;
    }
  },

  tokio_test_reenter_sync_call__async: true,
  tokio_test_reenter_sync_call__deps: ['$Asyncify'],
  tokio_test_reenter_sync_call: () => Asyncify.handleAsync(async () => {
    await null;
    return wasmExports.tokio_test_reenter_sync();
  }),
});
