// SPDX-License-Identifier: MIT OR Apache-2.0
// Per-network configuration for Members' door (testnet only).
// Program and collection IDs live here — never hard-coded in Main.qml or logic.mjs.
// QML-safe JavaScript: no BigInt, no optional chaining, no Java reserved words.

export var DEFAULT_NETWORK = "rc3"

export var NETWORKS = {
    rc3: {
        id: "rc3",
        label: "Logos test network (rc3)",
        testnet: true,
        channel: "",
        rpc: "https://lez.nameswallet.app",
        nftProgram: "CjrUpegdA1RjqDN43T8P19kAkwnD1quAkNkER8k3dw9G",
        gate: "",
        consumer: "",
        collection: "",
        collectionName: ""
    },
    testnet04: {
        id: "testnet04",
        label: "Logos testnet 0.4",
        testnet: true,
        channel: "",
        rpc: "",
        nftProgram: "",
        gate: "",
        consumer: "",
        collection: "",
        collectionName: ""
    }
}

var B58_RE = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/

/** True when `text` has the shape of a 32-byte base58 account or program id. */
export function isBase58Id(text) {
    return typeof text === "string" && B58_RE.test(text.trim())
}

function normalizeRpc(url) {
    return String(url || "").trim().toLowerCase().replace(/\/+$/, "")
}

/**
 * Pick the network matching the wallet's `get_config().zone`:
 * - Entries with a `channel` configured match by exact channel only.
 * - Entries with a missing `channel` in the config may match by exact RPC only.
 * - An unknown zone returns `null` (never falls back to rc3).
 */
export function networkFor(walletConfig) {
    var zone = walletConfig && walletConfig.zone ? walletConfig.zone : null
    if (!zone) return null
    var ch = zone.channel ? String(zone.channel).trim().toLowerCase() : ""
    var rpc = zone.rpc ? normalizeRpc(zone.rpc) : ""
    var keys = Object.keys(NETWORKS)
    if (ch) {
        for (var i = 0; i < keys.length; i++) {
            var entry = NETWORKS[keys[i]]
            if (entry.channel && entry.channel.trim().toLowerCase() === ch) return entry
        }
    }
    if (rpc) {
        for (var j = 0; j < keys.length; j++) {
            var byRpc = NETWORKS[keys[j]]
            if (!byRpc.channel && byRpc.rpc && normalizeRpc(byRpc.rpc) === rpc) return byRpc
        }
    }
    return null
}

/** Human-readable label for an unknown wallet zone, for error notices. */
export function describeUnknownZone(walletConfig) {
    var zone = walletConfig && walletConfig.zone ? walletConfig.zone : null
    if (!zone) return ""
    if (zone.name && String(zone.name).trim() !== "") return String(zone.name).trim()
    if (zone.rpc && String(zone.rpc).trim() !== "") return String(zone.rpc).trim()
    if (zone.channel && String(zone.channel).trim() !== "") return String(zone.channel).trim().slice(0, 12) + "…"
    return ""
}

/** Combine a per-network entry (or null when unknown) with optional user-entered overrides. */
export function resolveConfig(network, overrides) {
    var ov = overrides || {}
    var hasNet = !!network
    var net = network || {}
    var collection = String(ov.collection !== undefined && String(ov.collection).trim() !== "" ? ov.collection : (net.collection || "")).trim()
    var consumer = String(ov.consumer !== undefined && String(ov.consumer).trim() !== "" ? ov.consumer : (net.consumer || "")).trim()
    var collectionName = String(ov.collectionName !== undefined && String(ov.collectionName).trim() !== "" ? ov.collectionName : (net.collectionName || "")).trim()
    var nftProgram = hasNet ? String(net.nftProgram || "").trim() : ""
    var nftProgramValid = hasNet && isBase58Id(nftProgram)
    var collectionValid = isBase58Id(collection)
    var consumerValid = isBase58Id(consumer)
    return {
        known: hasNet,
        networkId: hasNet ? net.id : "unknown",
        networkLabel: hasNet ? net.label : "Network not set up",
        testnet: hasNet && net.testnet === true,
        rpc: hasNet ? (net.rpc || "") : "",
        nftProgram: nftProgram,
        nftProgramValid: nftProgramValid,
        gate: hasNet ? (net.gate || "") : "",
        consumer: consumer,
        collection: collection,
        collectionName: collectionName,
        collectionValid: collectionValid,
        consumerValid: consumerValid,
        readyOffchain: hasNet && nftProgramValid && collectionValid,
        readyOnchain: hasNet && nftProgramValid && collectionValid && consumerValid
    }
}
