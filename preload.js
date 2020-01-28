// All of the Node.js APIs are available in the preload process.
// It has the same sandbox as a Chrome extension.
const audio = require('./audio_player')
const vibrant = require('vibrant');

process.once('loaded', () => {
  global.audio = audio;
  global.vibrant = vibrant;
})