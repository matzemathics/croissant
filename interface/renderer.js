// This file is required by the index.html file and will
// be executed in the renderer process for that window.
// No Node.js APIs are available in this process because
// `nodeIntegration` is turned off. Use `preload.js` to
// selectively enable features needed in the rendering
// process.

window.addEventListener('load', () => {
    window.audio.init(() => { });
    console.log(document.getElementById('play'))
    document.getElementById('play').contentDocument.onclick = () => {
        window.audio.play();
    }
    console.log(document.getElementById("cover-image").src)
    window.audio.play();
})

