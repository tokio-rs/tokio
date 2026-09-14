// `--pre-js` for the emscripten test lanes. A test binary whose runtime never
// exits (a JSPI park with no wake source suspends `main` forever; an
// event-loop root that stalls leaves nothing armed) is drained by Node, which
// exits 0, which cargo would take as success.
var tokioRuntimeExited = false;
Module['onExit'] = () => { tokioRuntimeExited = true; };
process.on('exit', (code) => {
  if (code === 0 && !tokioRuntimeExited) {
    console.error('rt_emscripten_pre.js: event loop drained before the runtime exited');
    process.exitCode = 1;
  }
});
