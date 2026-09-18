// `--pre-js` for the emscripten test lanes. A test binary whose runtime never
// exits (a JSPI park with no wake source suspends `main` forever; an
// event-loop root that stalls leaves nothing armed) is drained by Node, which
// exits 0, which cargo would take as success.
//
// A binary that sets `Module.tokioExpectDone` must also set `Module.tokioDone`
// before the runtime exits: an event loop that stops holding the runtime too
// early exits cleanly with its roots unfinished.
// Immediates scheduled since load, for tests that bound them.
Module.tokioImmediates = 0;
if (globalThis.setImmediate) {
  const setImmediateHost = globalThis.setImmediate;
  globalThis.setImmediate = (...args) => {
    Module.tokioImmediates++;
    return setImmediateHost(...args);
  };
}
var tokioRuntimeExited = false;
Module['onExit'] = () => { tokioRuntimeExited = true; };
process.on('exit', (code) => {
  if (code === 0 && !tokioRuntimeExited) {
    console.error('rt_emscripten_pre.js: event loop drained before the runtime exited');
    process.exitCode = 1;
  }
  if (code === 0 && Module.tokioExpectDone && !Module.tokioDone) {
    console.error('rt_emscripten_pre.js: runtime exited before the roots completed');
    process.exitCode = 1;
  }
  if (code === 0 && Module.tokioDeadline && Date.now() > Module.tokioDeadline) {
    console.error('rt_emscripten_pre.js: the runtime outlived its work');
    process.exitCode = 1;
  }
});
