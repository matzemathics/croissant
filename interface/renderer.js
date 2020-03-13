
window.addEventListener('load', () => {
    audio.init();

    document.getElementById('play').onclick = play_action;
    document.getElementById('next').onclick = next_action;
    document.getElementById('prev').onclick = prev_action;

    document.getElementById('desc_btn').onclick = () => {
        const text = document.getElementById('desc_text');
        text.hidden = !text.hidden;
        document.getElementById('desc_btn').innerText = text.hidden ? "info" : "close";
    }

    document.getElementById('add_next').onclick = () => { 
        open_action(f => {
            audio.playlist.add_next(f);
            schedule_update();
        }); 
    }
    document.getElementById('add_queue').onclick = () => { open_action(audio.playlist.add_to_queue); }
    document.getElementById('add_m3u').onclick = () => { open_action(audio.playlist.import_m3u); }

    audio.control.pause();
    updateInfo();
})

var cover = new Cover();
var tag = new Tag();

function Cover () {
    this.song_path = null;
    this.cover_path = null;

    this.update = function (path) {
        if (this.song_path !== path) {
            this.song_path = path;
            const p = path.replace(/\/[^\/]*$/, "");

            const cover_files = ["/folder.jpg", "/cover.jpg", "/folder.png", "/cover.png"].map(x => p+x);

            for (const file of cover_files) {
                if (fs.existsSync(file)) {
                    document.getElementById("cover-image").src = file;
                    vibrate();
                    return;
                }
            }

            console.log("no cover");
            document.getElementById("cover-image").src="icons/Blank_CD_icon.png";
            setColor([216,191,216]);
        }
    }
}

function Tag () {
    this.curr = null;

    const tags = ['artist', 'album', 'title'];
    this.update = function (t) {
        if (!t) return;
        if ( !this.curr || tags.find(x => this.curr[x] !== t[x])) {
            this.curr = t;

            tags.forEach(x => {
                document.getElementById(x).textContent = t[x];
            })
            
            const cover_div = document.getElementById('cover');
            const tag_div = document.getElementById('tags');
            const tag_sep = document.getElementById('tag-seperator');
            const tag_br = document.getElementById('tag-br');

            if (cover_div && tag_div && tag_sep && tag_br) {
                if (cover_div.offsetWidth < tag_div.offsetWidth && tag_br.hidden) {
                    tag_sep.hidden = true;
                    tag_br.hidden = false;
                } else if (cover_div.offsetWidth > tag_div.offsetWidth && tag_sep.hidden) {
                    tag_sep.hidden = false;
                    tag_br.hidden = true;
                }
            }
        }
    }
}

function vibrate () {
    new Vibrant(document.getElementById("cover-image"))
        .getPalette((_, palette) => setColor(palette.LightMuted.rgb))
}

function setColor (c) {
    const rgb = ([r,g,b]) => `rgb(${r}, ${g}, ${b})`;
    document.getElementsByTagName("body")[0].style.backgroundColor = rgb(c);
    const sel = c.map(x => Math.sqrt(x * x * 0.7));
    document.getElementById("dynstyle").innerText = 
        `ol#playlist > li:hover { background: ${rgb(sel)}}`;
}

let info_timeout = null;

function updateInfo(){
    if (audio.info.changed()) {
        const info = audio.info.curr_info();
        cover.update(info.path);
        tag.update(info.tag);
        plSetId(info.id);
    }

    info_timeout = setTimeout(updateInfo, 3000);
}

function schedule_update () {
    clearTimeout(info_timeout);
    info_timeout = setTimeout(updateInfo, 100);
}

function plSetId (id) {
    document.getElementById("playlist").childNodes.forEach((x, i) => {
        if (i == id) x.firstChild.style.fontWeight = "bold";
        else x.firstChild.style.fontWeight = "normal";
    })
}

function updatePlaylist() {
    const playlist = audio.info.playlist();
    
    const node = document.getElementById("playlist");
    const pl_node = node.cloneNode(false);

    const sel_id = audio.info.curr_info().id;

    playlist.forEach((item, i) => {
        const li = document.createElement("li");
        ["title", "album", "artist"].forEach(x => {
            const span = document.createElement("span");
            span.className = "pl-" + x;
            span.innerText = item[x];
            li.appendChild(span);
        });
        li.onclick = () => {
            audio.playlist.skip_to(i);
            schedule_update();
        }
        if (i == sel_id) li.firstChild.style.fontWeight = "bold";
        pl_node.appendChild(li);
    });
    node.parentNode.replaceChild(pl_node, node);
}

function play_action () {
    audio.control.play();
    document.getElementById('play').src = "icons/pause.svg";
    document.getElementById('play').onclick = pause_action;
}

function pause_action () {
    audio.control.pause();
    document.getElementById('play').src = "icons/play.svg";
    document.getElementById('play').onclick = play_action;
}

function next_action () {
    audio.control.skip();
    schedule_update();
}

function prev_action () {
    audio.control.prev();
    schedule_update();
}

function open_action (f) {
    dialog.showOpenDialog().then(res => {
        res.filePaths.forEach(p => {
            f(p);
            updatePlaylist();
        })
    });
}