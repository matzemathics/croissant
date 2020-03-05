// This file is required by the index.html file and will
// be executed in the renderer process for that window.
// No Node.js APIs are available in this process because
// `nodeIntegration` is turned off. Use `preload.js` to
// selectively enable features needed in the rendering
// process.

window.addEventListener('load', () => {
    window.audio.init(() => { });
    console.log(document.getElementById('play'))

    document.getElementById('play').onclick = play_action;
    document.getElementById('next').onclick = next_action;
    document.getElementById('prev').onclick = prev_action;

    console.log(document.getElementById("cover-image").src)
    window.audio.pause();
    showImage();
})

var cover = new Cover();

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
                    console.log("file exists: " + file);
                    document.getElementById("cover-image").src = file;

                    vibrate();
                    return;
                }
            }

            console.log("no cover");

            //
        }
    }
}

function vibrate () {
    var vibrant = new Vibrant(document.getElementById("cover-image"));
    vibrant.getPalette((err, palette) => {
        document.getElementsByTagName("body")[0].style.backgroundColor = palette.LightVibrant.hex;
        console.log(palette);
    })
}

function showImage(){
    cover.update(window.audio.curr_playing());
    setTimeout(showImage, 1000);
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
}

function prev_action () {
    window.audio.prev();
}