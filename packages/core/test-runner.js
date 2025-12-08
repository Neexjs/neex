try {
  const runner = require('./dist/src/runner.js');
  console.log('Runner loaded:', runner);
} catch (e) {
  console.error('Runner load failed:', e);
}
