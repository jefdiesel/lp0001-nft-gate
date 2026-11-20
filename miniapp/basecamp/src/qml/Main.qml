// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door, Basecamp ui module SKELETON (spec §5). Not built, not published.
// Calls only the `membership-proof` methods of the names module (docs/WALLET-INTEGRATION.md).
// Logic (check, derivations, plain text) is in logic.mjs and is tested in node (miniapp/test).
// Chain reads need a C++ backend; that backend is not written here.
import QtQuick
import QtQuick.Layouts
import "logic.mjs" as Logic

Item {
    id: root
    property string requestId: ""
    property string stage: ""
    property string verdictText: ""
    property string nonce: ""
    property bool busy: false
    property int retries: 0
    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModuleAsync
    readonly property bool configured: config.collection.length > 0

    function call(method, args, done) {
        logos.callModuleAsync("names", method, args, function (reply) {
            var r = JSON.parse(reply)
            if (r.busy) {
                if (root.retries >= 6) {
                    root.busy = false
                    root.stage = "The wallet is busy. Try again in a moment."
                    return
                }
                root.retries++
                root.stage = "The wallet is busy; retrying (" + root.retries + "/6)…"
                retry.method = method; retry.args = args; retry.done = done; retry.start()
                return
            }
            root.retries = 0
            done(r)
        })
    }
    Timer { id: retry; interval: 500; property string method; property var args; property var done
            onTriggered: root.call(method, args, done) }

    function randomNonce() {   // QML has no crypto API: ask the wallet's module for randomness in the real app
        var s = ""; for (var i = 0; i < 64; i++) s += "0123456789abcdef"[Math.floor(Math.random() * 16)]
        return s
    }

    function request(onchain) {
        if (!bridgeReady || !configured || busy) return
        busy = true
        stage = "Preparing…"
        verdictText = ""
        nonce = randomNonce()
        var body = { collection: config.collection, purpose: "Check in this app", nonce: nonce, ttl: 3600000 }
        call("request_membership_proof", [JSON.stringify(body)], function (r) {
            busy = false
            if (!r.success) { stage = "Not done: " + (r.error || "the wallet refused the request"); return }
            requestId = r.value.request
            stage = "Waiting for you to approve in the wallet…"
            poll.start()
        })
    }

    // Status only while this page is open and in front (Qt.application.state).
    Timer { id: poll; interval: 5000; repeat: true
        running: false
        onTriggered: if (Qt.application.state === Qt.ApplicationActive) root.call("membership_proof_status", [requestId], function (r) {
            if (!r.success) {
                stage = "Could not read the request status: " + (r.error || "unknown error")
                poll.stop()
                return
            }
            var s = r.value
            stage = Logic.progressLine(s.state, s.phase, s.elapsed_seconds)
            if (s.state === "done") verdictText = Logic.proofLine(s.proof)
            if (["done", "failed", "rejected", "expired", "cancelled"].indexOf(s.state) >= 0) poll.stop()
        })
    }

    property var config: ({ collection: "", consumer: "" })   // the deployment config, pinned at build

    ColumnLayout {
        anchors.fill: parent; anchors.margins: 16; spacing: 12
        Text { text: "Members' door"; font.pixelSize: 22 }
        Text { text: "Prove you hold an NFT of the collection. Not shown: which NFT, which account."; wrapMode: Text.Wrap; Layout.fillWidth: true }

        Text {
            visible: !bridgeReady
            text: "The wallet bridge is not available here. Open this app in Basecamp with the Names module installed."
            wrapMode: Text.Wrap; Layout.fillWidth: true; color: "#a00"
        }
        Text {
            visible: bridgeReady && !configured
            text: "No collection is configured. Add one to this app's settings before checking in."
            wrapMode: Text.Wrap; Layout.fillWidth: true; color: "#a00"
        }

        RowLayout {
            Button {
                text: busy ? "Preparing…" : "Check in this app"
                enabled: bridgeReady && configured && !busy && !poll.running
                onClicked: request(false)
            }
            Button {
                text: "Sign the members' book on chain (not available yet)"
                enabled: false
                onClicked: {}
            }
        }
        Text { visible: !bridgeReady || !configured; text: "On-chain signing needs a pinned gate; this wallet does not have one yet."; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { text: stage; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { text: verdictText; wrapMode: Text.Wrap; Layout.fillWidth: true }
    }
}
EOF