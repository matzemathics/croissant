const addon = require('../native');

exports.init = addon.init;

exports.play = addon.play;
exports.pause = addon.pause;
exports.skip = addon.skip;
exports.prev = addon.prev;

exports.add_to_queue = addon.add_to_queue;
exports.import_m3u = addon.import_m3u;

exports.curr_info = () => ({
    tag: addon.curr_tag(),
    path: addon.curr_playing(),
    id: addon.curr_id()
})
exports.skip_to = addon.skip_to;
exports.playlist = addon.playlist;

exports.changed = addon.changed;