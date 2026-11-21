// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door, Basecamp ui module SKELETON (spec §5). Not built, not published.
// Calls only the `membership-proof` methods of the wallet (docs/WALLET-INTEGRATION.md). Finding
// the wallet, connecting, opening Names and the busy retry are the Names Wallet SDK's job
// (./NamesWallet.qml and ./NamesWalletSDK.mjs, copied unchanged): one timer per wait, no shared
// retry timer that a later busy answer could overwrite.
// Logic (check, derivations, plain text, the numbered steps) is in logic.mjs and is tested in
// node (miniapp/test). Chain reads need a C++ backend; that backend is not written here.
//
// The look is the shared kit (./kit): one left-aligned column, at most 680 px, 56 px from the
// edge (24 when narrow), PageHeader, one strong Action, verb + object labels, a plain result
// line, and a "How it works" card whose current step shows its help and its button. Every
// wallet call, argument and state change is unchanged from the earlier screen.

import QtQuick
import QtQuick.Layouts
import Logos.Theme
import "logic.mjs" as Logic
import "kit"

Item {
    id: root
    property Look look: Look {}

    // ── Wallet: the SDK finds it, connects, opens Names and retries busy calls ────────
    NamesWallet {
        id: wallet
        app: "members_door_ui"           // metadata.json `name`
        bridge: typeof logos !== "undefined" ? logos : null
        Component.onCompleted: getWallet(function(w) {
            if (w) check({})
        })
    }
    property string requestId: ""
    property string stage: ""
    property string verdictText: ""
    property string nonce: ""
    property bool busy: false
    readonly property bool bridgeReady: typeof logos !== "undefined" && !!logos.callModuleAsync && !!wallet.current
    readonly property bool configured: config.collection.length > 0

    // ── View state ───────────────────────────────────────────────────────────────────
    property bool guideOpen: true
    property bool guideDone: false
    readonly property bool showGuide: guideOpen || !guideDone
    readonly property bool proofReady: verdictText !== ""
    readonly property int step: Logic.doorStep({ proofReady: proofReady, waiting: poll.running, busy: busy })

    readonly property string networkName: "Logos test network"
    readonly property string networkDetail: "Names Wallet"

    function randomNonce() {   // QML has no crypto API: ask the wallet's module for randomness in the real app
        var s = ""; for (var i = 0; i < 64; i++) s += "0123456789abcdef"[Math.floor(Math.random() * 16)]
        return s
    }

    // The SDK's call() retries a busy answer with its own one-shot timer per call. The old shared
    // "The wallet is busy; retrying (n/6)…" line is now inside the SDK, not on this screen; the
    // plain final busy and refusal lines below are unchanged.
    function proofFailed(r) {
        busy = false
        if (r.busy) { stage = "The wallet is busy. Try again in a moment."; return }
        stage = "Not done: " + (r.error || "the wallet refused the request")
    }

    function sendProof(body) {
        wallet.call("request_membership_proof", [JSON.stringify(body)], function (v) {
            busy = false
            requestId = v.request
            stage = "Waiting for you to approve in the wallet…"
            poll.start()
            wallet.open()
        }, proofFailed)
    }

    function request(onchain) {
        if (!bridgeReady || !configured || busy) return
        busy = true
        stage = "Preparing…"
        verdictText = ""
        nonce = randomNonce()
        var body = { collection: config.collection, purpose: "Check in this app", nonce: nonce, ttl: 3600000 }
        if (wallet.state === "connected") { sendProof(body); return }
        wallet.connect("Check in this app", {}, {
            waiting: function(r) { stage = "Waiting for you to approve in the wallet…" },
            done: function(r) { sendProof(body) },
            failed: proofFailed
        })
    }

    // Status only while this page is open and in front (Qt.application.state).
    Timer { id: poll; interval: 5000; repeat: true
        running: false
        onTriggered: if (Qt.application.state === Qt.ApplicationActive) wallet.call("membership_proof_status", [requestId], function (s) {
            stage = Logic.progressLine(s.state, s.phase, s.elapsed_seconds)
            if (s.state === "done") verdictText = Logic.proofLine(s.proof)
            if (["done", "failed", "rejected", "expired", "cancelled"].indexOf(s.state) >= 0) poll.stop()
        }, function (r) {
            stage = "Could not read the request status: " + (r.error || "unknown error")
            poll.stop()
        })
    }

    property var config: ({ collection: "", consumer: "" })   // the deployment config, pinned at build

    function finishGuide() {
        guideOpen = false
        guideDone = true
    }

    // ── Screen ───────────────────────────────────────────────────────────────────────

    Rectangle { anchors.fill: parent; color: look.slate }

    Flickable {
        id: page
        anchors.fill: parent
        contentHeight: column.implicitHeight + 80
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        ColumnLayout {
            id: column
            x: root.width >= 600 ? look.edge : 24
            y: 40
            width: Math.min(root.width - 2 * x, look.pageWidth)
            spacing: 16

            RowLayout {
                Layout.fillWidth: true
                spacing: 16
                PageHeader {
                    look: root.look
                    title: "Members' door"
                    sentence: "Prove you hold an NFT of the collection without showing which NFT or which account."
                    runsOnPrefix: "Runs on"
                    network: root.networkName
                    networkDetail: root.networkDetail
                }
                Link {
                    look: root.look
                    Layout.alignment: Qt.AlignTop
                    font.underline: true
                    text: root.showGuide ? "Hide steps" : "Steps"
                    onPressed: root.showGuide ? root.finishGuide() : root.guideOpen = true
                }
            }

            // ── Before you start: a real problem, in red and in plain words ──────────
            Card {
                visible: !root.bridgeReady || !root.configured
                Heading { text: "Before you start" }
                Body {
                    visible: !root.bridgeReady
                    text: "The wallet bridge is not available here. Open this app in Basecamp with the Names module installed."
                    color: look.alarm
                }
                Body {
                    visible: root.bridgeReady && !root.configured
                    text: "No collection is configured. Add one to this app's settings before checking in."
                    color: look.alarm
                }
            }

            // ── The steps, with the current one marked and its help and button ───────
            Card {
                visible: root.showGuide
                Heading { text: "How it works" }
                Repeater {
                    model: Logic.doorSteps(root.step)
                    delegate: RowLayout {
                        id: stepRow
                        required property var modelData
                        readonly property bool now: modelData.state === "now"
                        Layout.fillWidth: true
                        Layout.topMargin: 6
                        spacing: 12
                        Rectangle {
                            Layout.alignment: Qt.AlignTop
                            Layout.preferredWidth: look.px(24); Layout.preferredHeight: look.px(24); radius: width / 2
                            color: stepRow.now ? look.brass : "transparent"
                            border.width: 1
                            border.color: stepRow.modelData.state === "later" ? look.line : look.brass
                            Text {
                                anchors.centerIn: parent
                                textFormat: Text.PlainText
                                text: stepRow.modelData.state === "done" ? "✓" : String(stepRow.modelData.n)
                                color: stepRow.now ? look.slate : look.ink
                                font.family: look.sans
                                font.pixelSize: look.px(13)
                                font.weight: Font.DemiBold
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Text {
                                textFormat: Text.PlainText
                                Layout.fillWidth: true
                                text: stepRow.modelData.text
                                color: stepRow.modelData.state === "later" ? look.dim : look.ink
                                font.family: look.sans
                                font.pixelSize: look.px(16)
                                font.weight: stepRow.now ? Font.DemiBold : Font.Normal
                                wrapMode: Text.Wrap
                            }
                            Body {
                                visible: stepRow.now
                                text: stepRow.modelData.n === 2 && root.poll.running
                                    ? "Names Wallet is open with the request: approve the proof. You come back here after."
                                    : stepRow.modelData.detail
                            }
                            Action {
                                visible: stepRow.now && stepRow.modelData.n === 1
                                look: root.look; strong: true; changes: true; busy: root.busy
                                label: "Check in this app"
                                busyLabel: root.poll.running ? "Waiting for you in Names…" : "Preparing…"
                                working: root.busy || root.poll.running
                                enabledNow: root.bridgeReady && root.configured && !root.busy && !root.poll.running
                                onPressed: root.request(false)
                            }
                            Link {
                                visible: stepRow.now && stepRow.modelData.n === 2 && root.poll.running
                                look: root.look
                                font.underline: true
                                text: "Check the status"
                                onPressed: root.poll.trigger()
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.topMargin: 4
                    spacing: 20
                    Action { look: root.look; label: "Got it"; visible: root.proofReady; onPressed: root.finishGuide() }
                }
            }

            // ── The one action when the steps are hidden ─────────────────────────────
            Card {
                visible: !root.showGuide && root.bridgeReady && root.configured && !root.proofReady
                Heading { text: "Check in here" }
                Body { text: "Names Wallet asks you to prove you hold an NFT of the collection. This app never sees your account or which NFT you use." }
                Action {
                    look: root.look; strong: true; changes: true; busy: root.busy
                    label: "Check in this app"
                    busyLabel: root.poll.running ? "Waiting for you in Names…" : "Preparing…"
                    working: root.busy || root.poll.running
                    enabledNow: !root.busy && !root.poll.running
                    onPressed: root.request(false)
                }
            }

            // ── The finished proof ────────────────────────────────────────────────────
            Card {
                visible: root.proofReady
                Heading { text: "Your pass" }
                Body { text: root.verdictText; color: look.ink }
            }

            // ── On-chain use, still unavailable: no call, no strong action ────────────
            Card {
                Heading { text: "On-chain use" }
                Action { look: root.look; label: "Sign the members' book"; enabledNow: false; onPressed: {} }
                Body { text: "On-chain signing needs a pinned gate; this wallet does not have one yet." }
            }

            TechDetails {
                look: root.look
                title: "Technical details"
                summary: root.configured ? "Collection " + Logic.short(root.config.collection) : ""
                FactRow { look: root.look; key: "Collection"; value: root.config.collection; monospace: true }
                FactRow { look: root.look; key: "Proof"; value: "Runs in Names Wallet. This app sees only the pass id and when it is valid." }
            }

            // The plain result of the last step. Only a real problem is red.
            Text {
                textFormat: Text.PlainText
                visible: root.stage !== ""
                Layout.fillWidth: true
                text: root.stage
                color: Logic.isProblem(root.stage) ? look.alarm : look.ink
                font.family: look.sans
                font.pixelSize: look.px(16)
                font.weight: Font.Medium
                wrapMode: Text.Wrap
            }
        }
    }

    // ── Parts of this screen (the shared ones are in ./kit) ──────────────────────────

    component Card: Rectangle {
        default property alias content: cardColumn.data
        Layout.fillWidth: true
        implicitHeight: cardColumn.implicitHeight + 36
        radius: 8
        color: root.look.card
        border.width: 1
        border.color: root.look.line
        ColumnLayout {
            id: cardColumn
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 18 }
            spacing: 10
        }
    }

    component Heading: Text {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        color: root.look.ink
        font.family: root.look.sans
        font.pixelSize: root.look.px(19)
        font.weight: Font.DemiBold
        wrapMode: Text.Wrap
    }

    component Body: Text {
        textFormat: Text.PlainText
        Layout.fillWidth: true
        color: root.look.dim
        font.family: root.look.sans
        font.pixelSize: root.look.px(15)
        lineHeight: 1.2
        wrapMode: Text.Wrap
    }
}
