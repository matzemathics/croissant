
//+---------------------------------------------------+
//|  renderer.js - Verwaltung des User-Interface      |
//+---------------------------------------------------+

// Initialisierung beim vollständigem Laden des Layouts
window.addEventListener('load', () => {
    // Initialisierung des Rust-Moduls
    audio.init();

    // Zuweisung der Ereignisbehandlung, sobald ein Button geklickt wird
    document.getElementById('play').onclick = play_action;
    document.getElementById('next').onclick = next_action;
    document.getElementById('prev').onclick = prev_action;

    document.getElementById('desc_btn').onclick = () => {
        const text = document.getElementById('desc_text');
        // Sichtbarkeit der Info-Box wird umgeschaltet
        text.hidden = !text.hidden;
        // Text des Buttons passt sich an
        document.getElementById('desc_btn').innerText = 
            text.hidden ? "info" : "close";
    }

    document.getElementById('add_next').onclick = () => {
        // Öffnen einer Datei
        open_action(f => {
            // Datei als nächstes abspielen
            audio.playlist.add_next(f);
            // Die momentan gespielte Datei ändert sich, 
            // also aktialisieren
            schedule_update();
        }); 
    }
    document.getElementById('add_queue').onclick = () => {
        // Datei öffnen und anhängen 
        open_action(audio.playlist.add_to_queue); 
    }
    document.getElementById('add_m3u').onclick = () => {
        // Playlist-datei öffnen und importieren 
        open_action(audio.playlist.import_m3u); 
    }

    // Pausieren der Audio-Wiedergabe, 
    // solange nichts gespielt wird
    audio.control.pause();

    // Beginne in regelmäßigen Abständen die Informationen
    // zu aktualisieren
    updateInfo();
})

// globale Variablen für Verwaltung von 
// Cover und aktuellen Informationen
var cover = new Cover();
var tag = new Tag();

// konstruiert Objekte, die das Cover verwalten
function Cover () {
    this.song_path = null;
    this.cover_path = null;

    this.update = function (p) {
        // aktualisiere nur bei Änderung
        if (this.song_path !== p) {
            this.song_path = p;
            const dir = path.parse(p).dir;

            // Liste bekannter Namen, um Cover-Bild zu suchen
            const cover_files = [
                "folder.jpg", 
                "cover.jpg", 
                "folder.png", 
                "cover.png"
            ];

            for (const file of cover_files.map(x => dir + path.sep + x)) {
                if (fs.existsSync(file)) {
                    // Cover gefunden!
                    document.getElementById("cover-image").src = file;
                    vibrate();
                    return;
                }
            }

            // falls kein Cover gefunden wurde
            document.getElementById("cover-image").src="icons/Blank_CD_icon.png";
            setColor([216,191,216]);
        }
    }
}

// konstruiert Objekte, die den Tag verwalten
function Tag () {
    this.curr = null;

    // Liste der interessanten tags
    const tags = ['artist', 'album', 'title'];

    this.update = function (t) {
        // wenn Argument null, beende
        if (!t) return;

        // aktualisiere nur wenn mindestens ein Tag sich geändert hat
        if ( !this.curr || tags.find(x => this.curr[x] !== t[x])) {
            this.curr = t;

            // Aktualisiere die Informationsfelder (unterhalb des Covers)
            tags.forEach(x => {
                document.getElementById(x).textContent = t[x];
            })
            
            const cover_div = document.getElementById('cover');
            const tag_div = document.getElementById('tags');
            const tag_sep = document.getElementById('tag-seperator');
            const tag_br = document.getElementById('tag-br');

            // Passe das Laout der Tags der Breite des Covers an
            if (cover_div && tag_div && tag_sep && tag_br) {
                if (cover_div.offsetWidth < tag_div.offsetWidth && tag_br.hidden) {
                    // Vertikales Layout

                    // <künstler>
                    //  <album>
                    //  <titel>

                    tag_sep.hidden = true;
                    tag_br.hidden = false;
                } else if (cover_div.offsetWidth > tag_div.offsetWidth && tag_sep.hidden) {
                    // Horizontales Layout

                    // <künstler> - <album>
                    //      <titel>

                    tag_sep.hidden = false;
                    tag_br.hidden = true;
                }
            }
        }
    }
}

// passe das Farbschema dem Cover an
function vibrate () {
    new Vibrant(document.getElementById("cover-image"))
        .getPalette((_, palette) => setColor(palette.LightMuted.rgb))
}

// setze Hintergrund und Auswahlfarbe
function setColor (c) {
    const rgb = ([r,g,b]) => `rgb(${r}, ${g}, ${b})`;
    document.getElementsByTagName("body")[0].style.backgroundColor = rgb(c);

    // Auswahlfarbe wird aus der Hintergrundfarbe berechnet
    const sel = c.map(x => Math.sqrt(x * x * 0.7));

    // berechnete Farben in das 
    // <style id="dynstyle"></style>
    // Tag einfügen
    document.getElementById("dynstyle").innerText = 
        `ol#playlist > li:hover { background: ${rgb(sel)}}`;
}

// timeout bis zur nächsten aktualisierung
let info_timeout = null;

function updateInfo(){
    // aktualisiere nur bei Änderung
    if (audio.info.changed()) {
        // info vom Rust-Modul erfragen
        const info = audio.info.curr_info();

        // info an die Objekte weitergeben
        cover.update(info.path);
        tag.update(info.tag);
        plSetId(info.id);
    }

    // setze 3-Sekunden-Timeout 
    // bis zur nächsten Aktualisierung
    info_timeout = setTimeout(updateInfo, 3000);
}

// updateInfo vorziehen 
function schedule_update () {
    clearTimeout(info_timeout);
    info_timeout = setTimeout(updateInfo, 100);
}

// setze das Element auf Fettdruck, welches gerade abgespielt wird
function plSetId (id) {
    document.getElementById("playlist").childNodes.forEach((x, i) => {
        if (i == id) x.firstChild.style.fontWeight = "bold";
        else x.firstChild.style.fontWeight = "normal";
    })
}

// aktualiiere Playlist
function updatePlaylist() {
    // lade Playlist au dem Rust-Modul
    const playlist = audio.info.playlist();
    
    // kopiere das Playlist Tag ohne dessen Inhalt
    const node = document.getElementById("playlist");
    const pl_node = node.cloneNode(false);

    // sel_id = id des momentan gespielten Songs
    const sel_id = audio.info.curr_info().id;

    playlist.forEach((item, i) => {
        // schreibe informationen in ein li Element
        const li = document.createElement("li");
        ["title", "album", "artist"].forEach(x => {
            // jedes Tag bekommt sein eigenes
            // <span class="pl-[tag]">[tag-content]</span>
            const span = document.createElement("span");
            span.className = "pl-" + x;
            span.innerText = item[x];
            li.appendChild(span);
        });
        
        // bei Click, spiele das jeweilige Element ab
        li.onclick = () => {
            audio.control.skip_to(i);
            schedule_update();
        }

        // das monentan gespielte Element wird fett gedruckt
        if (i == sel_id) li.firstChild.style.fontWeight = "bold";
        pl_node.appendChild(li);
    });

    // ersetze die alte durch die aktualisierte Playlist
    node.parentNode.replaceChild(pl_node, node);
}

// setze das Abspielen fort
function play_action () {
    audio.control.play();
    document.getElementById('play').src = "icons/pause.svg";
    document.getElementById('play').onclick = pause_action;
}

// halte die Musik an
function pause_action () {
    audio.control.pause();
    document.getElementById('play').src = "icons/play.svg";
    document.getElementById('play').onclick = play_action;
}

// springe einen Titel weiter
function next_action () {
    audio.control.skip();
    schedule_update();
}

// springe einen Titel zurück
function prev_action () {
    audio.control.prev();
    schedule_update();
}

// öffne eine Datei, übergib sie f 
// und aktualisiere die Playlist
function open_action (f) {
    dialog.showOpenDialog().then(res => {
        res.filePaths.forEach(p => {
            f(p);
            updatePlaylist();
        })
    });
}