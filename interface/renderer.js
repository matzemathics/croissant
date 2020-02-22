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

    console.log(document.getElementById("cover-image").src)
    window.audio.play();
})

function play_action () {
    window.audio.play();
    document.getElementById('play').src = "icons/pause.svg";
    document.getElementById('play').onclick = pause_action;
}

function pause_action () {
    window.audio.play();
    document.getElementById('play').src = "icons/play.svg";
    document.getElementById('play').onclick = play_action;
}