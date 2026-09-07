/*
 * `window.webxdc`, as the app running inside the WebView sees it.
 *
 * Served by the shim's own loopback host (webxdc_host.rs), which injects a
 * script tag for it into the app's index.html before anything of the app's
 * own runs -- the API has to be there when the first line of the app
 * executes, and `selfAddr` has to be readable without waiting.
 *
 * The placeholders are filled in by the host as JSON literals.
 *
 * Updates travel over the same host: sending is a POST, receiving is a
 * poll. Nothing here talks to Qt -- a page cannot -- and nothing here
 * knows the core; both are on the other side of those two requests.
 *
 * XMLHttpRequest rather than fetch: Gecko has both, and this one is
 * older than every release this could run on.
 */
(function () {
    "use strict";

    /* Where the chat is: `/webxdc-api/<token>`, which the host fills
     * in. The app's own files are at the root; this is the part of
     * the host nothing but the app is told about. */
    var API = __API__;
    var SELF_ADDR = __SELF_ADDR__;
    var SELF_NAME = __SELF_NAME__;
    var MAX_SIZE = __MAX_SIZE__;
    /* Milliseconds between polls for updates from the other end. */
    var POLL = __POLL__;

    /* What setUpdateListener was given, and how far it has been fed. */
    var listener = null;
    var lastSerial = 0;
    /* Resolves the promise setUpdateListener returned, once the updates
     * that existed when it was called have all been delivered. */
    var caughtUp = null;
    /* One poll at a time: a send asks for one immediately, and that must
     * not race the timer's. */
    var polling = false;
    var timer = null;

    function request(method, path, body, onDone, onFail) {
        var xhr = new XMLHttpRequest();
        xhr.open(method, API + path, true);
        xhr.onload = function () {
            if (xhr.status >= 200 && xhr.status < 300) {
                onDone(xhr.responseText);
            } else {
                onFail(new Error("webxdc: the app host answered " + xhr.status));
            }
        };
        xhr.onerror = function () {
            onFail(new Error("webxdc: the app host could not be reached"));
        };
        if (body === null) {
            xhr.send();
        } else {
            xhr.setRequestHeader("Content-Type", "application/json");
            xhr.send(body);
        }
    }

    function settle() {
        if (caughtUp) {
            var resolve = caughtUp;
            caughtUp = null;
            resolve();
        }
    }

    function deliver(updates) {
        for (var i = 0; i < updates.length; i++) {
            var update = updates[i];
            if (typeof update.serial === "number" && update.serial > lastSerial) {
                lastSerial = update.serial;
            }
            if (listener) {
                try {
                    listener(update);
                } catch (err) {
                    /* The app's own handler threw. Its problem, not the
                     * next update's: one that stopped the loop would
                     * leave the app frozen at the update it choked on. */
                    if (window.console) {
                        window.console.error(err);
                    }
                }
            }
        }
    }

    function schedule() {
        if (timer === null) {
            timer = window.setTimeout(function () {
                timer = null;
                poll();
            }, POLL);
        }
    }

    function poll() {
        if (polling) {
            return;
        }
        polling = true;
        request("GET", "/updates?serial=" + lastSerial, null,
            function (text) {
                polling = false;
                var updates = [];
                try {
                    updates = JSON.parse(text) || [];
                } catch (err) {
                    updates = [];
                }
                deliver(updates);
                settle();
                schedule();
            },
            function () {
                /* The host is gone -- the app was closed, or the shim
                 * stopped it. Keep asking: nothing else here can tell
                 * the difference between that and a lost connection,
                 * and an app left running with a dead listener is
                 * worse than a request that fails every POLL ms. */
                polling = false;
                settle();
                schedule();
            });
    }

    window.webxdc = {
        selfAddr: SELF_ADDR,
        selfName: SELF_NAME,
        /* The core's own limits, which apps are expected to respect. */
        sendUpdateInterval: __INTERVAL__,
        sendUpdateMaxSize: MAX_SIZE,

        /*
         * Send one update to everyone in the chat, including this app's
         * own listener -- the core echoes it back with a serial.
         *
         * `description` is accepted and ignored: the core dropped it, and
         * an app that passes one is not wrong for having been written
         * against the older API.
         */
        sendUpdate: function (update, description) {
            var body = JSON.stringify(update);
            if (MAX_SIZE > 0 && body.length > MAX_SIZE) {
                throw new Error("webxdc: update is larger than sendUpdateMaxSize");
            }
            request("POST", "/send", body,
                function () {
                    /* The core has it; ask for it back rather than
                     * waiting out the poll, so the app sees its own
                     * move at once. */
                    poll();
                },
                function (err) {
                    if (window.console) {
                        window.console.error(err);
                    }
                });
        },

        /*
         * Take every update from `serial` on, and every one after.
         * Resolves once the updates that already existed have been
         * handed over, which is what apps wait on before drawing.
         */
        setUpdateListener: function (callback, serial) {
            listener = callback;
            lastSerial = typeof serial === "number" ? serial : 0;
            return new Promise(function (resolve) {
                caughtUp = resolve;
                poll();
            });
        },

        /*
         * Put a file, a piece of text, or both into a chat.
         *
         * The specification says this asks the reader which chat, and
         * warns the app that it may not come back -- so what this sends
         * is a handover rather than a send: the host writes the file out
         * and the app's page asks which chat it is for. The promise
         * settles when the host has the file, which is the last moment
         * anything here still knows what happened.
         *
         * A file arrives in one of three shapes and leaves in one: a
         * Blob is read, text is encoded, and base64 is passed through.
         */
        sendToChat: function (message) {
            return new Promise(function (resolve, reject) {
                var content = message || {};
                var file = content.file;
                var words = typeof content.text === "string" ? content.text : "";
                if (!file && words.length === 0) {
                    reject(new Error("webxdc: nothing to send"));
                    return;
                }
                if (!file) {
                    handOver("", "", words, resolve, reject);
                    return;
                }
                if (typeof file.name !== "string" || file.name.length === 0) {
                    reject(new Error("webxdc: a file needs a name"));
                    return;
                }
                if (typeof file.base64 === "string") {
                    handOver(file.name, file.base64, words, resolve, reject);
                } else if (typeof file.plainText === "string") {
                    handOver(file.name, encode(file.plainText), words,
                             resolve, reject);
                } else if (file.blob) {
                    read(file.blob, function (encoded) {
                        handOver(file.name, encoded, words, resolve, reject);
                    }, reject);
                } else {
                    reject(new Error("webxdc: a file needs a blob, base64 or plainText"));
                }
            });
        }
    };

    /* Hand one over to the host, which writes it out and tells the page.
     */
    function handOver(name, encoded, words, resolve, reject) {
        var body = JSON.stringify({ name: name, base64: encoded, text: words });
        request("POST", "/to-chat", body, function () { resolve(); },
                function (err) { reject(new Error(err)); });
    }

    /* A Blob as base64. `readAsDataURL` rather than `readAsArrayBuffer`:
     * the browser does the encoding, and what comes back is
     * `data:<type>;base64,<payload>` with the payload after the comma. */
    function read(blob, done, fail) {
        var reader = new FileReader();
        reader.onload = function () {
            var url = "" + reader.result;
            var comma = url.indexOf(",");
            done(comma < 0 ? "" : url.slice(comma + 1));
        };
        reader.onerror = function () {
            fail(new Error("webxdc: the file could not be read"));
        };
        reader.readAsDataURL(blob);
    }

    /* Text as base64, through UTF-8: `btoa` takes bytes, and
     * `encodeURIComponent` is the way to get them out of a string
     * without a TextEncoder, which this engine may not have. */
    function encode(text) {
        var utf8 = unescape(encodeURIComponent(text));
        return window.btoa(utf8);
    }

    /* Nothing else is offered: importFiles and joinRealtimeChannel are
     * absent rather than present and failing, so an app that
     * feature-tests for them takes its own other path. */
}());
