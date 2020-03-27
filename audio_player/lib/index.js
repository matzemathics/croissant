
//+---------------------------------------------------+
//|  index.js - diese Datei beinhaltet die JavaScript | 
//|             Schnittstelle für das Rust-Modul      |
//+---------------------------------------------------+

// laden des Rust-Packetes (../native/index.node)
const addon = require('../native');

// die init() Funktion wird übernommen
exports.init = addon.init;

// das control Objekt enthält die Funtionen für
// play(), pause(), skip(), prev() und skip_to()
exports.control = {
    play: addon.play,
    pause: addon.pause,
    skip: addon.skip,
    prev: addon.prev,
    skip_to: addon.skip_to
}

// das info Objekt fasst Funktionen zum aktuellen
// Status des Programs zusammen
exports.info = {
    // curr_info() - informationen über den momentanen Titel
    curr_info: () => ({
        tag: addon.curr_tag(),          // Infos über Titel, Künstler, Album
        path: addon.curr_playing(),     // Dateipfad des aktuellen Titels
        id: addon.curr_id()             // Position in der Playlist
    }),
    // changed() - haben sich die Informationen seit dem
    //             letzten Aufruf geändert
    changed: addon.changed,
    // playlist() - vollständige Auskunf über alle Titel in der
    //              Playlist
    playlist: addon.playlist
}

// Funtionen, die die Playlist verändern
exports.playlist = {
    add_to_queue: addon.add_to_queue,   // Datei hinten an der Playlist anhängen
    add_next: addon.add_next,           // Datei vorne Anhängen (unterbricht aktuellen Titel)
    import_m3u: addon.import_m3u        // Playlist importieren
}