// This file is required by the index.html file and will
// be executed in the renderer process for that window.
// No Node.js APIs are available in this process because
// `nodeIntegration` is turned off. Use `preload.js` to
// selectively enable features needed in the rendering
// process.

window.addEventListener('load', () => {
    window.audio.init();

    document.getElementById('play').onclick = play_action;
    document.getElementById('next').onclick = next_action;
    document.getElementById('prev').onclick = prev_action;

    document.getElementById('desc_btn').onclick = () => {
        const text = document.getElementById('desc_text');
        text.hidden = !text.hidden;
        document.getElementById('desc_btn').innerText = text.hidden ? "info" : "close";
    }

    window.audio.pause();
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

            //TODO: add no cover.
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
    var vibrant = new Vibrant(document.getElementById("cover-image"));
    vibrant.getPalette((err, palette) => {
        document.getElementsByTagName("body")[0].style.backgroundColor = palette.LightVibrant.hex;
    })
}

let info_timeout = null;

function updateInfo(){

    if (audio.changed()) {
        const info = audio.curr_info();
        cover.update(info.path);
        tag.update(info.tag);
        plSetId(info.id);
    }

    info_timeout = setTimeout(updateInfo, 3000);
}

function plSetId (id) {
    document.getElementById("playlist").childNodes.forEach((x, i) => {
        if (i == id) x.firstChild.style.fontWeight = "bold";
        else x.firstChild.style.fontWeight = "normal";
    })
}

function updatePlaylist() {
    const playlist = audio.playlist();
    
    const node = document.getElementById("playlist");
    const pl_node = node.cloneNode(false);

    playlist.forEach((item, i) => {
        const li = document.createElement("li");
        ["title", "album", "artist"].forEach(x => {
            const span = document.createElement("span");
            span.className = "pl-" + x;
            span.innerText = item[x];
            li.appendChild(span);
        });
        li.onclick = () => {
            audio.skip_to(i);
            clearTimeout(info_timeout);
            info_timeout = setTimeout(updateInfo, 100);
        }
        pl_node.appendChild(li);
    });
    node.parentNode.replaceChild(pl_node, node);
}

function play_action () {
    window.audio.play();
    document.getElementById('play').src = "icons/pause.svg";
    document.getElementById('play').onclick = pause_action;
}

function pause_action () {
    window.audio.pause();
    document.getElementById('play').src = "icons/play.svg";
    document.getElementById('play').onclick = play_action;
}

function next_action () {
    window.audio.skip();
    clearTimeout(info_timeout);
    info_timeout = setTimeout(updateInfo, 100);
}

function prev_action () {
    window.audio.prev();
    clearTimeout();
    info_timeout = setTimeout(updateInfo, 100);
}