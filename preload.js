// All of the Node.js APIs are available in the preload process.
// It has the same sandbox as a Chrome extension.
const audio = require('./audio_player')
const fs = require('fs');

process.once('loaded', () => {
  global.audio = audio;
  global.fs = fs;
})