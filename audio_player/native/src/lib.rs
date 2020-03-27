
//+-------------------------------------------------------------+
//| lib.rs - enthält alle von außen zugreifbaren Funktionen des |
//|          Rust-Moduls, verwaltet die unterschiedlichen       |
//|          Threads und den gemeinsamen Zustand                |
//+-------------------------------------------------------------+

extern crate neon;

use neon::prelude::*;

// registriert die aus JavaScript zugreifbaren Funtionen
register_module!(mut cx, {
    cx.export_function("play", play)
      .and(cx.export_function("pause", pause))
      .and(cx.export_function("skip", abort_curr))
      .and(cx.export_function("prev", prev))
      .and(cx.export_function("init", init))
      .and(cx.export_function("add_to_queue", add_pl))
      .and(cx.export_function("add_next", add_next))
      .and(cx.export_function("import_m3u", import_m3u))
      .and(cx.export_function("curr_playing", curr_playing))
      .and(cx.export_function("curr_tag", curr_tag))
      .and(cx.export_function("curr_id", curr_id))
      .and(cx.export_function("playlist", playlist))
      .and(cx.export_function("changed", changed))
      .and(cx.export_function("skip_to", skip_to))
});

// importiere Bibiliotheken (crates)

extern crate cpal;              // Zugriff auf Audiogeräte
extern crate samplerate;        // Resampling der Audiodateien
extern crate ringbuf;           // Ringbuffer um Audiodaten zu übertragen
extern crate lazy_static;       // Verwaltung des gemeinsamen Zustands
extern crate m3u;               // Unterstützung für .m3u Dateien (Playlists)
extern crate futures_util;      // Hilfsfunktionen für das arbeiten mit asynchronen Vorgängen

// Modul für das Lesen der Audiodateien (siehe dort)
mod audio_reader;

// use - wird in Rust benutzt, um anzugeben, welche der Funktionen und
//       Objekte direkt zugreifbar sind, ohne das crate oder Modul anzugeben

use audio_reader::buffered_reader::{BufferedReader, ReaderTarget};
use audio_reader::{AudioFile, AudioProducer, resample_read, Tags, Tagged};

use cpal::traits::{HostTrait, EventLoopTrait};
use cpal::{StreamData, UnknownTypeOutputBuffer, Format};

use std::{
    thread, thread::sleep, time::Duration, fmt::Debug,
    sync::{ mpsc::{channel, Sender}, Arc, Mutex },
    marker::PhantomData, collections::VecDeque
};

use futures::{
    executor::block_on,
    task::AtomicWaker,
    future::{Abortable, AbortHandle}
};

use ringbuf::{ RingBuffer, Producer };

use lazy_static::lazy_static;

use m3u::{Entry, EntryReader, Url};


//+--------------------------------
//| struct Playerstate<'a>
//|     - diese Struktur speichert
//|       den momentanen Zustand
//|       des Programms

struct PlayerState<'a> {
    // player: handhabt die Audiogeräte
    player: Option<CpalPlayer<'a>>,

    // play_queue: Liste der noch zu spielenden Titel
    play_queue: VecDeque<String>,
    
    // played_list: Liste der bereits gespielten Titel
    played_list: Vec<String>,
    
    // curr: momentan gespielter Titel
    curr: Option<(String, Tags, AbortHandle)>,

    // changed: Veränderung seit der letzten Kontrolle
    changed: bool
}

impl<'a> PlayerState<'a> {
    // erzeugt einen neuen, leeren Zustand
    fn new() -> PlayerState<'a> {
        PlayerState { 
            player: None, 
            played_list: Vec::new(), 
            play_queue: VecDeque::new(), 
            curr: None,
            changed: false
        }
    }

    // initialisiert den Zustand mit einem CpalPlayer (siehe dort)
    fn init(&mut self, player: CpalPlayer<'a>) {
        self.player = Some(player);
    }

    // bricht momentanen Titel ab
    fn abort_curr(&mut self) {
        if let Some((_, _, handle)) = &self.curr {
            handle.abort();
        }

        if let Some(player) = &self.player {
            player.clear_buffer();
        }
    }

    // Dateiname des nächsten Songs in der Playlist
    fn next(&self) -> Option<String> { 
        self.play_queue.front().cloned() 
    }

    // entfernt das erste Element der Playlist und setzt den aktuellen Titel
    fn advance(&mut self, info: Option<(String, Tags, AbortHandle)>) {
        let _ = self.play_queue.pop_front();

        if let Some((l, _, _)) = &self.curr {
            self.played_list.push(l.clone()) 
        }
        self.curr = info;
        self.changed = true;
    }

    // gibt an, ob sich seit dem letzten Aufruf informationen verändert haben
    fn changed (&mut self) -> bool {
        let c = self.changed;
        self.changed = false;
        c
    }

    // hängt den momentanen und zuletzt gespielten Titel an die playlist an
    // bricht momentane Wiedergabe ab
    fn go_back(&mut self) {
        if let Some((curr, _, _)) = self.curr.clone() {
            self.play_queue.push_front(curr);

            if let Some(last) = self.played_list.pop().clone() {
                self.play_queue.push_front(last);
            }

            self.abort_curr();

            self.curr = None;
        }
    }

    // gibt den Dateinamen des aktuellen Titels an
    fn curr_playing(&self) -> Option<String> {
        self.curr.as_ref().map(|(t, _, _)| t.clone())
    }

    // gibt die Informationen (Künstler, Album, Titel) des aktuellen Titels an
    fn curr_tags(&self) -> Option<Tags> {
        self.curr.as_ref().map(|(_, t, _)| t.clone())
    }

    // gibt die Position des aktuellen Titels an
    fn curr_id(&self) -> u32 {
        self.played_list.len() as u32
    }

    // gibt die Informationen aller Titel in der Playlist an
    fn tags(&self) -> Vec<Tags> {
        let mut res = Vec::new();

        for f in self.played_list.iter() {
            if let Some(file) = AudioFile::open(f.as_str()) {
                res.push(file.tags());
            }
        }
        if let Some(tags) = self.curr_tags() {
            res.push(tags);
        }
        for f in self.play_queue.iter() {
            if let Some(file) = AudioFile::open(f.as_str()) {
                res.push(file.tags());
            }
        }

        res
    }

    // hängt eine Datei an die Playlist an
    fn add_to_queue(&mut self, title: String) {
        self.play_queue.push_back(title);
    }

    // hängt eine Datei vorne an die Playlist an
    fn add_next(&mut self, title: String) {
        if let Some((p, _, _)) = &self.curr {
            self.play_queue.push_front(p.clone());
        }
        self.play_queue.push_front(title);
        self.abort_curr();
        self.curr = None;
    }

    // entfernt den aktuellen Titel aus der Playlist
    fn rm_curr (&mut self) {
        self.played_list.pop();
    }

    // springt zu einer beliebigen Position in der Playlist
    fn skip_to(&mut self, id: u32) {
        let mut curr = self.curr_id();

        let total_count = 
            self.played_list.len() 
            + self.play_queue.len() 
            + if self.curr.is_some() {1} else {0};

        if curr == id { return; }
        if id as usize > total_count { return; }

        // es muss rückwärts gesprungen werden
        if curr > id {
            // aktuellen Titel entfernen
            if let Some((t, _, _)) = &self.curr {
                self.play_queue.push_front(t.clone());
                curr -= 1;
            }
            // weiter zurück, bis zum gewünschten Titel
            for _ in id .. curr+1 {
                self.play_queue.push_front(self.played_list.pop().unwrap());
            }
        } 
        else {
            // aktuellen Titel entfernen
            if let Some((t, _, _)) = &self.curr {
                self.played_list.push(t.clone());
                curr += 1;
            }
            // weiter bis zum gewünschten Titel
            for _ in curr .. id {
                self.played_list.push(self.play_queue.pop_front().unwrap());
            }
        }

        // momentanen Song abbrechen
        if let Some((_, _, handle)) = &self.curr {
            handle.abort();
        }

        if let Some(player) = &self.player {
            player.clear_buffer();
        }

        self.curr = None;
    }
}

//+--------------------------------
//| struct CpalPlayer<'a>
//|     - diese Struktur handhabt
//|       die Audio Hardware

struct CpalPlayer<'a> {
    // handle für die Kontrolle des Audio-Threads
    event_loop: Arc<cpal::EventLoop>,
    // id des Audio-Streaams
    stream_id: cpal::StreamId,
    // gibt an ob gerade Musik abgespielt wird
    playing: bool,
    // Kommunikation mit dem Audio-Thread (bei Abbruch)
    channel: Sender<()>,
    // enthält keine Daten, nur für das Rust-Typensystem vorhanden
    phantom: PhantomData<&'a ()>
}

// Ausgabe von Fehlermeldungen auf der Konsole
fn stringify<T: Debug>(x: T) -> String { format!("Error - Debug: {:?}", x) }

impl CpalPlayer<'_> {
    // erzeugt einen neuen Audio-Player
    fn new<'a>(
        sample_rate: u32, 
        mut cons: ringbuf::Consumer<f32>,
        shared_waker: Arc<AtomicWaker>
    ) -> Option<CpalPlayer<'a>>
    {
        // Standard-Gerät vom System erfragen
        let host = cpal::default_host();
        let event_loop = Arc::new(host.event_loop());
        let device = host.default_output_device()?;

        // festlegen des Sampleformats
        let format = Format {
            channels: 2,
            sample_rate: cpal::SampleRate(sample_rate),
            data_type: cpal::SampleFormat::F32
        };

        // Kommunikations-Kanal zum Abbruch von Dateien
        let (send, recv) = channel();

        // Audio-Stream erstellen
        let stream_id = event_loop.build_output_stream(&device, &format).ok()?;
        event_loop.play_stream(stream_id.clone()).unwrap();

        // Kopie des event_loop für Verschiebung in den Audio-Thread
        let event_loop_copy = event_loop.clone();
        
        // erstellen des Audio-Threads
        // alle Variablen, die innerhalb des Threads
        // verwendet werden, werden von Rust in den Thread
        // verschoben und sind von außen nicht mehr zugreifbar
        thread::spawn(move || {
            event_loop_copy.run(move |stream_id, stream_result| {
                if cons.is_empty() {
                    // zur entlastung des Prozessors 200 ms warten
                    sleep(Duration::from_millis(200));
                }

                // audio buffer aus dem Argument erhalten
                let stream_data = match stream_result {
                    Ok(data) => data,
                    Err(err) => {
                        eprintln!("an error occurred on stream {:?}: {}", stream_id, err);
                        return;
                    }
                };

                // falls ein Abbruch erfragt wurde
                if recv.try_recv().is_ok() {
                    // ringbuffer leeren
                    cons.pop_each(|_| true, None);
                }

                match stream_data {
                    StreamData::Output { buffer: UnknownTypeOutputBuffer::F32(mut buffer) } => {
                        // schreibt den inhalt des ringbuffers (cons) in den buffer
                        for elem in buffer.iter_mut() {
                            *elem = cons.pop().unwrap_or(0.0);
                        }

                        // dem Erzeuger der Daten signalisieren, dass
                        // weitere Daten in den Buffer geschrieben werden können
                        shared_waker.wake();
                    },
                    _ => (),
                }
            });
        });
    
        Some(CpalPlayer {
            event_loop: event_loop,
            stream_id: stream_id,
            playing: true,
            channel: send,
            phantom: PhantomData
        })
    }

    // Audio-Thread über Abbruch benachrichtigen
    fn clear_buffer(&self) {
        self.channel.send(()).unwrap();
    }

    // Audio-Thread aufwecken
    fn play(&mut self) -> Result<(), String> {
        self.playing = true;
        self.event_loop.play_stream(self.stream_id.clone()).map_err(stringify)
    }

    // Audio-Thread pausieren
    fn pause(&mut self) -> Result<(), String> {
        self.playing = false;
        self.event_loop.pause_stream(self.stream_id.clone()).map_err(stringify)
    }
}

// erzeugt eine globale Variable STATE,
// diese ist aus allen Threads zugreifbar
lazy_static! { 
    static ref STATE: Arc<Mutex<PlayerState<'static>>> = {
        Arc::new(Mutex::new(PlayerState::new()))
    }; 
}

// implementiere die Schnittstelle (trait) ReaderTarget<T>
// für ringbuf::Producer<T> (aus ringbuf)
// siehe audio_reader/buffered_reader.rs
impl<T: Send> ReaderTarget<T> for ringbuf::Producer<T> {
    fn read_iter <I: Iterator<Item=T>> (&mut self, iter: &mut I) -> usize {
        self.push_iter(iter)
    }

    fn is_full (&self) -> bool {
        self.is_full()
    }
}

// diese Funktion erzeugt einen Thread, der
// mit STATE interagiert um nach und nach die
// Dateien in der PlayList in den Ringbuffer
// zu schreiben (prod: Producer<f32>)
fn spawn_file_reader (prod: Producer<f32>, sample_rate: u32, shared_waker: Arc<AtomicWaker>) {
    thread::spawn(move || {
        // speichere den RingBuffer innerhalb einer
        // Thread-sicheren Datenstruktur
        let prod = Arc::new(Mutex::new(prod));
        loop {
            // zugriff auf den globalen Zustand erhalten
            let mut guard = STATE.lock().unwrap();
            // nächste Date erfragen
            let next = guard.next();

            // Falls keine Datei in der Playlist
            if next.is_none() {
                std::mem::drop(guard);
                // auf anhängen neuer Datei warten
                sleep(Duration::from_millis(100));
                continue;
            }

            // Datei öffnen (s. audio_reader/mod.rs)
            let file_name = next.unwrap();
            let opened = AudioFile::open(file_name.as_str());

            // Falls Datei nicht geöffnet werden kann
            if opened.is_none() {
                // Datei aus Playlist entfernen
                guard.rm_curr();
                continue;
            }

            let mut file = opened.unwrap();
            // Tags aus der Datei lesen
            let tags = file.tags();

            // Datei-Leser konstruieren (s. audio_reader/buffered_reader.rs)
            let reader = BufferedReader::new(prod.clone(), shared_waker.clone());
            // asynchronen Lesevorgang beginnen (s. audio_reader/mod.rs)
            let future = resample_read(&mut file, reader, sample_rate);

            // einbetten des Lesevorgangs in einen abbrechbaren Vorgang
            let (abort_handle, abort_reg) = AbortHandle::new_pair();
            let future = Abortable::new(future, abort_reg);
            
            // aktualisieren des globalen Zustands
            guard.advance(Some((file_name.clone(), tags, abort_handle)));
            drop(guard);

            // für Debugging-Zwecke Dateinamen auf der Konsole ausgeben
            println!("playing {}", file_name);
            let _ = block_on(future);
        }
    });
}

//+------------------------------------------------------------------------------
//| JavaScript-Interface-Functions
//|     - Die Folgenden Funktionen können aus JavaScript aufgerufen werden.
//|       Sie sind die Schnittstelle des Rustmoduls zum Benutzer.

// startet die Threads - initialisiert das Rust-Modul
fn init(mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();

    // tue nichts, falls bereits initialisiert
    if state.player.is_some() { return Ok(cx.null()); }

    // Erstelle den Ringbuffer
    let sample_rate = 48000;
    let buffer_size = sample_rate as usize * 4;
    let auddiobuf = RingBuffer::<f32>::new(buffer_size);
    let (prod, cons) = auddiobuf.split();

    // Erstelle Waker zur benachrichtung des BufferedReader,
    // wenn wieder Daten geschrieben werden können
    let shared_waker = Arc::new(AtomicWaker::new());

    // Starte den Datei-Thread
    spawn_file_reader(prod, sample_rate, shared_waker.clone());

    // Initialisiere den Player
    let mut player = CpalPlayer::new(sample_rate, cons, shared_waker).expect("cannot open player");
    player.pause().unwrap();

    // initialisiere den globalen Zustand
    state.init(player);

    Ok(cx.null())
}

// Setze abspielen fort
fn play(mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();
    match &mut state.player {
        None => init(cx),
        Some(player) => {
            player.play().unwrap();
            Ok(cx.null())
        }
    }
}

// Pausiere das abspielen
fn pause(mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();
    match &mut state.player {
        None => init(cx),
        Some(player) => {
            player.pause().unwrap();
            Ok(cx.null())
        }
    }
}

// importiere eine Playlist
fn import_m3u (mut cx: FunctionContext) -> JsResult<JsNull> {
    if let Ok(arg0) = cx.argument::<JsString>(0) {
        // lese Datei
        let path = arg0.value();
        let mut reader = EntryReader::open(path.clone()).unwrap();
        // base: Dateipfad zur Playlist
        let base = Url::from_file_path(path.as_str()).unwrap();

        // Lies einträge aus der m3u-Datei
        for entry in reader.entries() {
            if let Ok(Entry::Path(p)) = entry {
                let p = base.join((*p).to_str().unwrap()).unwrap().to_file_path().unwrap();
                STATE.lock().unwrap().add_to_queue(p.to_str().unwrap().to_string());
            }
        }
    }
    Ok(cx.null())
}

// hänge Datei an die Playlist an
fn add_pl (mut cx: FunctionContext) -> JsResult<JsNull> {
    if let Ok(arg0) = cx.argument::<JsString>(0) {
        STATE.lock().unwrap().add_to_queue(arg0.value());
    }
    Ok(cx.null())
}

// spiele Datei als nächstes
fn add_next (mut cx: FunctionContext) -> JsResult<JsNull> {
    if let Ok(arg0) = cx.argument::<JsString>(0) {
        STATE.lock().unwrap().add_next(arg0.value());
    }
    Ok(cx.null())
}

// brich die aktuelle Datei ab
fn abort_curr (mut cx: FunctionContext) -> JsResult<JsNull> {
    STATE.lock().unwrap().abort_curr();
    Ok(cx.null())
}

// brich ab und spiele die latzte Datei ab
fn prev (mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();
    state.go_back();
    Ok(cx.null())
}

// springe zu einer beliebigen Datei in der Playlist
fn skip_to (mut cx: FunctionContext) -> JsResult<JsNull> {
    let mut state = STATE.lock().unwrap();

    if let Ok(arg) = cx.argument::<JsNumber>(0) {
        state.skip_to(arg.value() as u32);
    }

    Ok(cx.null())
}

// gib den Dateipfad der aktuellen Datei zurück
fn curr_playing (mut cx: FunctionContext) -> JsResult<JsValue> {
    let p = STATE.lock().unwrap().curr_playing();

    if let Some(path) = p {
        let res = cx.string(path);
        Ok(res.as_value(&mut cx))
    } else {
        let res = cx.null();
        Ok(res.as_value(&mut cx))
    }
}

// gib die Position der aktuellen Datei zurück
fn curr_id (mut cx: FunctionContext) -> JsResult<JsNumber> {
    let s = STATE.lock().unwrap();

    Ok(cx.number(s.curr_id()))
}

// Hilfsfunktion, konvertiert ein Tags Objekt (s. audio_reader/mod.rs)
// in ein JavaScript Objekt
fn tag_to_js<'a, C: Context<'a>> (cx: &mut C, t: Tags) -> Handle<'a, JsObject> {
    let res = cx.empty_object();

    // leere Strings als null ansehen
    let mut str_or_null = |x| {
        if x == "" {
            let res = cx.null();
            res.as_value(cx)
        } else {
            let res = cx.string(x);
            res.as_value(cx)
        }
    };

    let artist = str_or_null(t.artist());
    let album =  str_or_null(t.album());
    let title =  str_or_null(t.title());
    res.set(cx, "artist", artist).unwrap();
    res.set(cx, "album", album).unwrap();
    res.set(cx, "title", title).unwrap();

    res
}

// erzeugt ein JavaScript Array, das Informationen über die Playlist liefert
fn playlist(mut cx: FunctionContext) -> JsResult<JsArray> {
    let tags = STATE.lock().unwrap().tags();

    let array = cx.empty_array();

    for i in 0 .. tags.len() {
        let tag = tag_to_js(&mut cx, tags[i].clone());
        array.set(&mut cx, i as u32, tag).unwrap();
    }

    Ok(array)
}

// gibt Informationen über den momentanen Titel zurück
fn curr_tag (mut cx: FunctionContext) -> JsResult<JsValue> {
    let state = STATE.lock().unwrap();

    if let Some(t) = state.curr_tags() {
        let res = tag_to_js(&mut cx, t);
        return Ok(res.as_value(&mut cx));
    }

    let res = cx.null();
    Ok(res.as_value(&mut cx))
}

// gibt an, ob sich informationen seit dem letzten Aufruf geändert haben
fn changed (mut cx: FunctionContext) -> JsResult<JsBoolean> {
    Ok(cx.boolean(STATE.lock().unwrap().changed()))
}

