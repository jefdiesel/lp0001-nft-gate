// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door, Basecamp ui module SKELETON (spec §5). Not built, not published.
// Calls only the `membership-proof` methods of the names module (docs/WALLET-INTEGRATION.md),
// which the Names Wallet does not have yet. Logic (check, derivations) is in logic.mjs, tested
// in node (miniapp/test). Chain reads need a C++ backend like the Testimonial app's
// `accountView`; that backend is not written here.
import QtQuick
import QtQuick.Layouts
import "logic.mjs" as Logic

Item {
    id: root
    property string requestId: ""
    property string stage: ""
    property string verdictText: ""
    property string nonce: ""
    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModuleAsync

    function call(method, args, done) {
        logos.callModuleAsync("names", method, args, function (reply) {
            var r = JSON.parse(reply)
            if (r.busy) { retry.method = method; retry.args = args; retry.done = done; retry.start(); return }
            done(r)
        })
    }
    Timer { id: retry; interval: 1000; property string method; property var args; property var done
            onTriggered: root.call(method, args, done) }

    function randomNonce() {   // QML has no crypto API: ask the wallet's module for randomness in the real app
        var s = ""; for (var i = 0; i < 64; i++) s += "0123456789abcdef"[Math.floor(Math.random() * 16)]
        return s
    }

    function request(onchain) {
        stage = "Preparing…"
        nonce = randomNonce()
        var body = { collection: config.collection, purpose: "Check in this app", nonce: nonce, ttl: 3600000 }
        if (onchain) body.onchain = { consumer: config.consumer }
        call("request_membership_proof", [JSON.stringify(body)], function (r) {
            if (!r.success) { stage = "Not done: " + r.error; return }
            requestId = r.value.request
            poll.start()
        })
    }

    // Status only while this page is open and in front (Qt.application.state).
    Timer { id: poll; interval: 5000; repeat: true
        running: false
        onTriggered: if (Qt.application.state === Qt.ApplicationActive) root.call("membership_proof_status", [requestId], function (r) {
            if (!r.success) return
            var s = r.value
            stage = s.state === "running" ? ("Working: " + s.phase) : s.state
            if (s.state === "done" || s.state === "failed" || s.state === "rejected" || s.state === "expired" || s.state === "cancelled") poll.stop()
            // On done: read the slot through the backend, then Logic.check(state, binding, chainTimeMs).
        })
    }

    property var config: ({ collection: "", consumer: "" })   // the deployment config, pinned at build

    ColumnLayout {
        anchors.fill: parent; anchors.margins: 16; spacing: 12
        Text { text: "Members' door"; font.pixelSize: 22 }
        Text { text: "Prove you hold an NFT of the collection. Not shown: which NFT, which account."; wrapMode: Text.Wrap; Layout.fillWidth: true }
        RowLayout {
            Button { text: "Check in this app"; enabled: bridgeReady && !poll.running; onClicked: request(false) }
            Button { text: "Sign the members' book on chain"; enabled: bridgeReady && !poll.running; onClicked: request(true) }
        }
        Text { text: stage; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { text: verdictText; wrapMode: Text.Wrap; Layout.fillWidth: true }
    }
}
