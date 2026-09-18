// `--pre-js` for the JSPI lane. A test that suspends forever (a park with no
// wake source) leaves `main` pending; Node then exits 0 once its event loop
// drains, which cargo would take as success.
var tokioMainReturned = false;
Module['onExit'] = () => { tokioMainReturned = true; };
process.on('exit', (code) => {
  if (code === 0 && !tokioMainReturned) {
    console.error('rt_emscripten_pre.js: event loop drained with main still suspended');
    process.exitCode = 1;
  }
});
