//+-----------------------------------------------+
//| preload.js - laden der Module, die aus        |
//|              dem User-Interface zugreifbar    |
//|              sein müssen                      |
//+-----------------------------------------------+

const audio = require('./audio_player')
const fs = require('fs');
const {dialog} = require('electron').remote;
const path = require('path');

process.once('loaded', () => {
  global.audio = audio;
  global.fs = fs;
  global.dialog = dialog;
  global.path = path;
})