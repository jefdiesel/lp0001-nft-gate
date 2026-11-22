// SPDX-License-Identifier: MIT OR Apache-2.0
// Members' door (LP-0001): Basecamp QML view.
// Works with zero account access: calls only get_config and the membership-proof methods of
// Names Wallet (Names 0.18.3 provider_proof.rs contract). Never connects to or reads user
// accounts or balances.
// Styled with the Names Wallet / Vest kit (./kit): left-aligned column next to Basecamp's menu,
// at most 680 px wide, readable type, one strong action, immediate feedback on every press.

import QtQuick
import QtQuick.Layouts
import Logos.Theme
import "logic.mjs" as Logic
import "networks.mjs" as Networks
import "kit"

Item {
    id: root
    property Look look: Look {}

    // ── Wallet: find the wallet and read its config only (no account access) ────────
    NamesWallet {
        id: wallet
        app: "members_door_ui"
        bridge: typeof logos !== "undefined" ? logos : null
        onUpdated: if (wallet.status === "choose") root.checkingWallet = false
        Component.onCompleted: root.initWallet(false)
    }

    property var activeNetwork: null
    property bool configLoaded: false
    property string walletZoneLabel: ""
    property string collectionOverride: ""
    property string consumerOverride: ""
    readonly property var resolved: Networks.resolveConfig(activeNetwork, {
        collection: collectionOverride,
        consumer: consumerOverride
    })

    // ── Request & proof state (all ms timestamps use `real`) ────────────────────────
    property string requestId: ""
    property string activeMode: ""        // "offchain" | "onchain" | ""
    property string statusKind: ""        // "" | preparing | waiting | checking | proving | sending | sent | unverified | refused
    property string statusBadge: ""
    property string stage: ""
    property string statusReadWarning: ""
    property string nonce: ""
    property bool busy: false
    property bool provingBegan: false
    property bool checkingWallet: false
    property bool walletFeedbackPending: false
    property bool cancelling: false
    property bool canCancel: false
    property real ttlMs: 3600000
    property real startedAtMs: 0
    property real phaseAnchorMs: 0
    property real elapsedSeconds: 0
    property var lastProof: null

    readonly property bool hasBridge: typeof logos !== "undefined" && (!!logos.callModuleAsync || !!logos.callModule)
    readonly property bool bridgeReady: hasBridge && !!wallet.current && wallet.status !== "missing" && wallet.status !== "none" && wallet.status !== "choose"
    readonly property bool networkReady: configLoaded && resolved.known && resolved.nftProgramValid
    readonly property bool requestActive: poll.running || busy
    readonly property bool proofReturned: statusKind === "unverified" && lastProof !== null
    readonly property string walletLabel: wallet.current && wallet.current.display_name ? wallet.current.display_name : "Names Wallet"
    readonly property string collectionLabel: resolved.collectionName !== "" ? resolved.collectionName : (resolved.collection !== "" ? Logic.shortId(resolved.collection) : "")
    readonly property var passInfo: proofReturned ? Logic.describePass(lastProof, collectionLabel) : null

    // ── Guide state ─────────────────────────────────────────────────────────────────
    property bool guideOpen: true
    property bool guideDone: false
    readonly property bool showGuide: guideOpen || !guideDone
    readonly property int step: Logic.doorStep({ proofReturned: proofReturned, waiting: poll.running, busy: busy })

    function initWallet(fromUser) {
        if (!hasBridge) return
        checkingWallet = true
        if (fromUser) walletFeedbackPending = true
        wallet.getWallet(function(w) {
            if (!w) {
                checkingWallet = false
                configLoaded = false
                activeNetwork = null
                walletZoneLabel = ""
                if (walletFeedbackPending) {
                    walletFeedbackPending = false
                    statusKind = "refused"
                    statusBadge = "Refused"
                    stage = "Not done: " + (wallet.problem !== "" ? wallet.problem : "Names Wallet is not installed.")
                }
                return
            }
            wallet.call("get_config", [], function(cfg) {
                checkingWallet = false
                configLoaded = true
                wallet.config = cfg
                activeNetwork = Networks.networkFor(cfg)
                walletZoneLabel = Networks.describeUnknownZone(cfg)
                if (walletFeedbackPending) {
                    walletFeedbackPending = false
                    if (!activeNetwork) {
                        statusKind = "refused"
                        statusBadge = "Network not set up"
                        stage = "This network is not set up in Members' door yet."
                    } else {
                        statusKind = ""
                        statusBadge = ""
                        stage = root.walletLabel + " is ready."
                    }
                }
            }, function(r) {
                checkingWallet = false
                configLoaded = false
                activeNetwork = null
                walletZoneLabel = ""
                if (walletFeedbackPending) {
                    walletFeedbackPending = false
                    statusKind = "refused"
                    statusBadge = "Refused"
                    stage = "Not done: " + (r && r.error ? r.error : "Names Wallet is not answering.")
                }
            })
        })
    }

    function retryWallet() {
        if (!hasBridge || checkingWallet) return
        activeMode = ""
        statusReadWarning = ""
        statusKind = "preparing"
        statusBadge = "Waiting"
        stage = "Looking for Names Wallet…"
        wallet.forgetWallet()
        initWallet(true)
    }

    // NOTE: Math.random() is predictable, not cryptographic randomness. Qt QML has no
    // crypto.getRandomValues(), so this nonce is only a session placeholder and does not
    // provide replay protection until a native or module CSPRNG supplies 32 random bytes.
    function randomNonce() {
        var hexChars = "0123456789abcdef"
        var out = ""
        for (var i = 0; i < 64; i++) out += hexChars[Math.floor(Math.random() * 16)]
        return out
    }

    function currentSession() {
        return {
            polling: poll.running,
            provingBegan: provingBegan,
            statusKind: statusKind,
            statusBadge: statusBadge,
            stage: stage,
            canCancel: canCancel,
            elapsedSeconds: elapsedSeconds,
            lastProof: lastProof,
            statusReadWarning: statusReadWarning,
            actionsEnabled: !poll.running && !busy
        }
    }

    function applySession(next) {
        provingBegan = next.provingBegan
        statusKind = next.statusKind
        statusBadge = next.statusBadge
        stage = next.stage
        canCancel = next.canCancel
        elapsedSeconds = next.elapsedSeconds
        lastProof = next.lastProof
        statusReadWarning = next.statusReadWarning
        if (!next.polling) {
            busy = false
            poll.stop()
            clockTick.stop()
        }
    }

    function applyStatus(record) {
        var nowMs = Date.now()
        var nested = record && record.status && typeof record.status === "object" ? record.status : null
        var hasWalletElapsed = nested && nested.state === "proving" && nested.elapsed_seconds !== undefined && nested.elapsed_seconds !== null && !isNaN(Number(nested.elapsed_seconds))
        var fallbackElapsed = 0
        if (hasWalletElapsed) {
            fallbackElapsed = Math.max(0, Math.floor(Number(nested.elapsed_seconds)))
            phaseAnchorMs = nowMs - fallbackElapsed * 1000
        } else if (nested && nested.state === "proving") {
            if (phaseAnchorMs <= 0) phaseAnchorMs = nowMs
            fallbackElapsed = Math.max(0, Math.floor((nowMs - phaseAnchorMs) / 1000))
        } else {
            phaseAnchorMs = 0
            fallbackElapsed = 0
        }
        var next = Logic.applyPollSuccess(currentSession(), record, fallbackElapsed, activeMode === "onchain")
        applySession(next)
    }

    function startRequest(onchain) {
        if (!bridgeReady || !networkReady || busy || poll.running) return
        busy = true
        cancelling = false
        provingBegan = false
        statusReadWarning = ""
        activeMode = onchain ? "onchain" : "offchain"
        statusKind = "preparing"
        statusBadge = "Waiting"
        stage = "Preparing…"
        lastProof = null
        canCancel = false
        elapsedSeconds = 0
        startedAtMs = Date.now()
        phaseAnchorMs = 0
        nonce = randomNonce()

        var built = Logic.buildProofRequest({
            program: resolved.nftProgram,
            collection: resolved.collection,
            purpose: onchain ? "Sign the members' book" : "Check in this app",
            nonce: nonce,
            ttl: ttlMs,
            onchain: onchain,
            consumer: resolved.consumer
        })
        if (!built.ok) {
            busy = false
            statusKind = "refused"
            statusBadge = "Refused"
            stage = "Not done: " + built.error
            return
        }

        // No account access: only the membership-proof method is called.
        wallet.call("request_membership_proof", [JSON.stringify(built.body)], function(v) {
            busy = false
            if (!v || !v.request) {
                statusKind = "refused"
                statusBadge = "Refused"
                stage = "Not done: Names Wallet returned no request id. Nothing was sent."
                return
            }
            requestId = String(v.request)
            startedAtMs = Date.now()
            phaseAnchorMs = 0
            provingBegan = false
            statusReadWarning = ""
            statusKind = "waiting"
            statusBadge = "Waiting"
            stage = Logic.progressLine("awaiting-approval", 0)
            canCancel = true
            poll.start()
            clockTick.start()
            wallet.open()
        }, function(r) {
            busy = false
            statusKind = "refused"
            statusBadge = "Refused"
            canCancel = false
            if (r && r.busy) {
                stage = "The wallet is busy. Try again in a moment."
                return
            }
            if (r && r.uncertain) {
                stage = r.error || Logic.UNKNOWN_FAILURE_LINE
                return
            }
            if (r && r.missing) {
                stage = "Not done: " + (r.error || "Names Wallet is not open.")
                return
            }
            var plain = onchain
                ? Logic.onchainRefusalMessage("", r ? r.error : "", { beforeId: true })
                : Logic.plainError("", r ? r.error : "", { beforeId: true })
            stage = plain === Logic.UNKNOWN_FAILURE_LINE ? plain : ("Not done: " + plain)
        })
    }

    function pollOnce(manual) {
        if (!requestId) return
        if (manual) statusReadWarning = ""
        // Quiet mode avoids trapping wallet.status in "missing" on a transient status read error.
        // A failed status read is NOT a failed proof: keep polling and never re-enable actions.
        wallet.call("membership_proof_status", [String(requestId)], function(s) {
            applyStatus(s)
        }, function(r) {
            applySession(Logic.applyPollFailure(currentSession(), r))
        }, { quiet: true })
    }

    function cancelActive() {
        if (!requestId || cancelling) return
        cancelling = true
        statusReadWarning = ""
        wallet.call("cancel_membership_proof", [String(requestId)], function(s) {
            cancelling = false
            applyStatus(s)
        }, function(r) {
            cancelling = false
            if (r && r.busy) {
                statusReadWarning = "Names Wallet is busy. Try cancelling again in a moment."
                return
            }
            if (r && r.error && /can't be cancelled/i.test(String(r.error))) {
                statusReadWarning = "This proof has moved past the stage where it can be cancelled. Still checking its status."
                return
            }
            statusReadWarning = "Could not cancel the request. Still checking its status."
        }, { quiet: true })
    }

    // Status poll runs only while this app is active and in front.
    Timer {
        id: poll
        interval: 5000
        repeat: true
        running: false
        onTriggered: if (Qt.application.state === Qt.ApplicationActive) root.pollOnce(false)
    }

    // Smooth 1-second display tick for elapsed time while proving.
    Timer {
        id: clockTick
        interval: 1000
        repeat: true
        running: false
        onTriggered: {
            if (root.phaseAnchorMs <= 0) return
            if (root.statusKind === "proving") {
                root.elapsedSeconds = Math.max(0, Math.floor((Date.now() - root.phaseAnchorMs) / 1000))
                root.stage = Logic.progressLine("proving", root.elapsedSeconds)
            }
        }
    }

    function finishGuide() {
        guideOpen = false
        guideDone = true
    }

    // ── Screen (left-aligned column next to Basecamp's menu, matching Vest & Names) ──

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
            width: Math.max(0, Math.min(root.width - 2 * x, look.pageWidth))
            spacing: 16

            RowLayout {
                Layout.fillWidth: true
                spacing: 16
                PageHeader {
                    look: root.look
                    title: "Members' door"
                    sentence: "Prove you hold an NFT of a collection without showing which NFT or which account."
                    runsOnPrefix: "Runs on"
                    network: root.configLoaded ? root.resolved.networkLabel : "Checking network…"
                    networkDetail: root.configLoaded && root.resolved.known ? (root.walletLabel + " · testnet only") : root.walletLabel
                }
                Link {
                    look: root.look
                    Layout.alignment: Qt.AlignTop
                    text: root.showGuide ? "Hide steps" : "Steps"
                    onPressed: root.showGuide ? root.finishGuide() : root.guideOpen = true
                }
            }

            // ── Wallet / bridge availability (real problems only) ────────────────────
            Card {
                visible: !root.hasBridge || wallet.status === "none" || wallet.status === "missing" || wallet.status === "choose"
                Heading { text: wallet.status === "choose" ? "Choose a wallet" : "Before you start" }
                Body {
                    visible: !root.hasBridge
                    text: "Open this app in Basecamp with Names Wallet installed."
                    color: look.alarm
                }
                Body {
                    visible: root.hasBridge && (wallet.status === "none" || wallet.status === "missing")
                    text: wallet.problem !== "" ? wallet.problem : "Names Wallet is not open. Install or open Names in Basecamp, then try again."
                    color: look.alarm
                }
                RowLayout {
                    visible: root.hasBridge && (wallet.status === "none" || wallet.status === "missing")
                    spacing: 12
                    Action {
                        look: root.look
                        label: "Try again"
                        busyLabel: "Checking…"
                        busy: root.checkingWallet
                        working: root.checkingWallet
                        enabledNow: !root.checkingWallet
                        onPressed: root.retryWallet()
                    }
                }
                Body {
                    visible: root.hasBridge && wallet.status === "choose"
                    text: "More than one wallet is installed. Pick the one Members' door should ask for proofs."
                }
                Repeater {
                    model: wallet.status === "choose" ? wallet.wallets : []
                    delegate: Action {
                        required property var modelData
                        look: root.look
                        label: "Use " + modelData.display_name
                        onPressed: {
                            root.activeMode = ""
                            root.statusReadWarning = ""
                            root.walletFeedbackPending = true
                            root.statusKind = "preparing"
                            root.statusBadge = "Waiting"
                            root.stage = "Connecting to " + modelData.display_name + "…"
                            wallet.chooseWallet(modelData.module)
                        }
                    }
                }
            }

            // ── Unknown network notice (never falls back to rc3) ─────────────────────
            Card {
                visible: root.bridgeReady && root.configLoaded && !root.resolved.known
                Heading { text: "Network not set up" }
                Body {
                    text: root.walletZoneLabel !== ""
                        ? ("This network (" + root.walletZoneLabel + ") is not set up in Members' door yet. Both actions are disabled. Switch Names Wallet to a supported test network, then try again.")
                        : "This network is not set up in Members' door yet. Both actions are disabled. Switch Names Wallet to a supported test network, then try again."
                    color: look.alarm
                }
                RowLayout {
                    spacing: 12
                    Action {
                        look: root.look
                        label: "Try again"
                        busyLabel: "Checking…"
                        busy: root.checkingWallet
                        working: root.checkingWallet
                        enabledNow: !root.checkingWallet && !root.requestActive
                        onPressed: root.retryWallet()
                    }
                }
            }

            // ── Numbered steps (collapsible) ─────────────────────────────────────────
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
                        Layout.topMargin: 4
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
                            spacing: 4
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
                                text: stepRow.modelData.n === 2 && poll.running
                                    ? "Names Wallet is open with the request. Approve the proof there, then come back here."
                                    : stepRow.modelData.detail
                            }
                        }
                    }
                }
                RowLayout {
                    visible: root.proofReturned
                    Layout.topMargin: 4
                    spacing: 16
                    Action { look: root.look; label: "Got it"; onPressed: root.finishGuide() }
                }
            }

            // ── Primary use: Check in this app (off-chain proof, no account access) ──
            Card {
                Heading { text: "Check in this app" }
                Body {
                    visible: root.configLoaded && root.resolved.known && !root.resolved.nftProgramValid
                    text: "No NFT program is set up for " + root.resolved.networkLabel + " yet. Both actions are disabled until one is configured in networks.mjs."
                    color: look.alarm
                }
                Body {
                    text: root.resolved.collectionValid
                        ? "Ask Names Wallet to prove you hold an NFT of " + root.collectionLabel + ". No account access is requested."
                        : "Enter the collection id below, then ask Names Wallet to prove you hold one of its NFTs. No account access is requested."
                }
                Field {
                    Layout.fillWidth: true
                    look: root.look
                    code: true
                    hint: root.activeNetwork && root.activeNetwork.collection
                        ? "Override collection id (default: " + Logic.shortId(root.activeNetwork.collection) + ")"
                        : "Collection id (base58)"
                    text: root.collectionOverride
                    onTextChanged: if (text !== root.collectionOverride) root.collectionOverride = text
                    onAccepted: if (root.bridgeReady && root.networkReady && root.resolved.readyOffchain && !root.requestActive) root.startRequest(false)
                }
                Body {
                    visible: root.collectionOverride.trim() !== "" && !root.resolved.collectionValid
                    text: "That does not look like a base58 collection id (32 to 44 characters)."
                    color: look.alarm
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12
                    Action {
                        look: root.look
                        strong: true
                        changes: true
                        busy: root.requestActive
                        working: root.activeMode === "offchain" && root.requestActive
                        label: root.proofReturned ? "Check in again" : "Check in this app"
                        busyLabel: Logic.buttonBusyLabel(root.statusKind, root.elapsedSeconds)
                        enabledNow: root.bridgeReady && root.networkReady && root.resolved.readyOffchain && !root.requestActive
                        onPressed: root.startRequest(false)
                    }
                }
            }

            // ── Live request status (waiting, checking, proving, sending, sent, unchecked, refused) ──
            Card {
                visible: root.stage !== "" || root.statusReadWarning !== ""
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Rectangle {
                        visible: root.statusBadge !== ""
                        implicitWidth: badgeText.implicitWidth + 18
                        implicitHeight: 26
                        radius: 13
                        color: "transparent"
                        border.width: 1
                        border.color: root.statusKind === "refused" ? look.alarm
                            : root.statusKind === "unverified" ? look.line
                            : look.brass
                        Text {
                            id: badgeText
                            anchors.centerIn: parent
                            textFormat: Text.PlainText
                            text: root.statusBadge
                            color: root.statusKind === "refused" ? look.alarm
                                : root.statusKind === "unverified" ? look.ink
                                : look.brass
                            font.family: look.sans
                            font.pixelSize: look.px(13)
                            font.weight: Font.DemiBold
                        }
                    }
                    Heading {
                        text: root.activeMode === "onchain" ? "On-chain proof status"
                            : root.activeMode === "offchain" ? "Proof status"
                            : "Status"
                    }
                }
                Text {
                    visible: root.stage !== ""
                    textFormat: Text.PlainText
                    Layout.fillWidth: true
                    text: root.stage
                    color: (root.statusKind === "refused" || Logic.isProblem(root.stage)) ? look.alarm : look.ink
                    font.family: look.sans
                    font.pixelSize: look.px(16)
                    font.weight: Font.Medium
                    wrapMode: Text.Wrap
                }
                Body {
                    visible: root.statusReadWarning !== ""
                    text: root.statusReadWarning
                    color: look.alarm
                }
                RowLayout {
                    visible: poll.running
                    Layout.topMargin: 2
                    spacing: 18
                    Link {
                        look: root.look
                        text: "Check status"
                        onPressed: root.pollOnce(true)
                    }
                    Link {
                        visible: root.canCancel && !root.cancelling
                        look: root.look
                        text: "Cancel request"
                        onPressed: root.cancelActive()
                    }
                }
            }

            // ── Returned pass details (neutral; NOT checked on chain by this app yet) ──
            Card {
                visible: root.proofReturned && root.passInfo !== null && root.passInfo.shapeValid
                Heading { text: "Pass returned by Names" }
                FactRow { look: root.look; key: "Status"; value: root.passInfo ? root.passInfo.statusLine : ""; tone: look.ink }
                FactRow { look: root.look; key: "Pass id"; value: root.passInfo ? root.passInfo.passIdShort : ""; monospace: true }
                FactRow { look: root.look; key: "Window"; value: root.passInfo ? root.passInfo.windowLine : "" }
                FactRow {
                    visible: root.passInfo && root.passInfo.onchainLine !== ""
                    look: root.look
                    key: "On chain"
                    value: root.passInfo ? root.passInfo.onchainLine : ""
                }
                FactRow { look: root.look; key: "Reveals"; value: root.passInfo ? root.passInfo.reveals : "" }
                FactRow { look: root.look; key: "Does not reveal"; value: root.passInfo ? root.passInfo.doesNotReveal : "" }
                Body { text: root.passInfo ? root.passInfo.trustNote : "" }
            }

            // ── Secondary use: On-chain ("Sign the members' book") ───────────────────
            Card {
                Heading { text: "On-chain use" }
                Body {
                    text: "Sign the members' book on chain using a private membership proof. When supported, you pick the public acting account inside Names Wallet."
                }
                Body {
                    text: "Current Names Wallet builds refuse on-chain proof requests until the wallet adds the acting-account picker and pinned gate."
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12
                    Action {
                        look: root.look
                        changes: true
                        busy: root.requestActive
                        working: root.activeMode === "onchain" && root.requestActive
                        label: "Sign the members' book"
                        busyLabel: Logic.buttonBusyLabel(root.statusKind, root.elapsedSeconds)
                        enabledNow: root.bridgeReady && root.networkReady && root.resolved.readyOnchain && !root.requestActive
                        onPressed: root.startRequest(true)
                    }
                }
                Body {
                    visible: !root.resolved.consumerValid
                    text: "Add a consumer program id in Technical details (or networks.mjs) to try an on-chain request."
                }
            }

            // ── What a pass reveals and what it does not ─────────────────────────────
            Card {
                Heading { text: "What a pass reveals" }
                FactRow {
                    look: root.look
                    key: "Reveals"
                    value: "That you hold an NFT of this collection, a pass id, and when it starts and ends. Two apps shown the same pass id can tell it is the same holder, not who you are. On-chain use also names the public acting account you pick in Names."
                }
                FactRow {
                    look: root.look
                    key: "Does not reveal"
                    value: "Which NFT you hold, which account holds it, your balances, or your other NFTs. This app never asks for account access."
                }
                FactRow {
                    look: root.look
                    key: "Limits"
                    value: "Proving takes about 10 minutes on a 16 GB computer and holds about 5 million fee reserve (mostly returned). Testnet only."
                }
            }

            // ── Technical details (per-network config & full ids) ────────────────────
            TechDetails {
                look: root.look
                title: "Technical details"
                summary: !root.resolved.known
                    ? "Network not set up"
                    : root.resolved.collectionValid
                        ? root.resolved.networkId + " · Collection " + Logic.shortId(root.resolved.collection)
                        : root.resolved.networkId + " · No collection set"
                FactRow { look: root.look; key: "Network"; value: root.resolved.known ? (root.resolved.networkLabel + " (testnet only)") : "Not set up in networks.mjs" }
                FactRow { look: root.look; key: "NFT program"; value: root.resolved.nftProgram !== "" ? root.resolved.nftProgram : "Not set in networks.mjs"; monospace: root.resolved.nftProgram !== "" }
                FactRow { look: root.look; key: "Gate"; value: root.resolved.gate !== "" ? root.resolved.gate : "Pinned by Names Wallet"; monospace: root.resolved.gate !== "" }
                FactRow { look: root.look; key: "Collection"; value: root.resolved.collection !== "" ? root.resolved.collection : "Not set"; monospace: root.resolved.collection !== "" }
                FactRow { look: root.look; key: "Consumer"; value: root.resolved.consumer !== "" ? root.resolved.consumer : "Not set"; monospace: root.resolved.consumer !== "" }
                FactRow {
                    visible: root.passInfo !== null && root.passInfo.passIdFull !== ""
                    look: root.look
                    key: "Pass id"
                    value: root.passInfo ? root.passInfo.passIdFull : ""
                    monospace: true
                }
                FactRow {
                    visible: root.lastProof !== null && !!root.lastProof.gate
                    look: root.look
                    key: "Returned gate"
                    value: root.lastProof && root.lastProof.gate ? String(root.lastProof.gate) : ""
                    monospace: true
                }
                FactRow {
                    visible: root.lastProof !== null && !!root.lastProof.tx_hash
                    look: root.look
                    key: "Prove tx"
                    value: root.lastProof && root.lastProof.tx_hash ? String(root.lastProof.tx_hash) : ""
                    monospace: true
                }
                FactRow {
                    visible: root.lastProof !== null && !!root.lastProof.act_tx_hash
                    look: root.look
                    key: "Act tx"
                    value: root.lastProof && root.lastProof.act_tx_hash ? String(root.lastProof.act_tx_hash) : ""
                    monospace: true
                }
                Body { text: "Consumer program id for on-chain use (defaults live in networks.mjs):" }
                Field {
                    Layout.fillWidth: true
                    look: root.look
                    code: true
                    hint: "Consumer program id for on-chain use (base58)"
                    text: root.consumerOverride
                    onTextChanged: if (text !== root.consumerOverride) root.consumerOverride = text
                }
                Body {
                    visible: root.consumerOverride.trim() !== "" && !root.resolved.consumerValid
                    text: "That does not look like a base58 consumer program id (32 to 44 characters)."
                    color: look.alarm
                }
                FactRow { look: root.look; key: "Proof"; value: "Runs in Names Wallet. This app asks for membership-proof only, never account access." }
            }
        }
    }

    // ── Parts of this screen (shared kit is in ./kit) ───────────────────────────────

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
