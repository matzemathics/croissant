const addon = require('../native');

exports.init = addon.init;

exports.control = {
    play: addon.play,
    pause: addon.pause,
    skip: addon.skip,
    prev: addon.prev
}

exports.info = {
    curr_info: () => ({
        tag: addon.curr_tag(),
        path: addon.curr_playing(),
        id: addon.curr_id()
    }),
    changed: addon.changed,
    playlist: addon.playlist
}

exports.playlist = {
    skip_to: addon.skip_to,
    add_to_queue: addon.add_to_queue,
    add_next: addon.add_next,
    import_m3u: addon.import_m3u
}