package com.tchirek.locosaonduh.engine

import android.content.Context
import android.os.Build
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

class SavedListen(
    val peerId: String,
    val kind: String,
    val alias: String,
    val source: String,
) {
    fun json(): JSONObject = JSONObject()
        .put("peerId", peerId)
        .put("kind", kind)
        .put("alias", alias)
        .put("source", source)

    fun same(other: SavedListen): Boolean =
        peerId == other.peerId && kind == other.kind && alias == other.alias && source == other.source

    companion object {
        fun from(raw: JSONObject): SavedListen = SavedListen(
            peerId = raw.optString("peerId"),
            kind = raw.optString("kind", "system"),
            alias = raw.optString("alias", "locoSaonduh"),
            source = raw.optString("source", "音频"),
        )
    }
}

// 秋風を 戸に書き留む 名のごとし
class Settings(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var id: String = prefs.getString("id", null).orEmpty()
    var alias: String = prefs.getString("alias", null).orEmpty()
    var outputId: String? = prefs.getString("outputId", null)
    var micId: String? = prefs.getString("micId", null)
    var systemOn: Boolean = prefs.getBoolean("systemOn", false)
    var micOn: Boolean = prefs.getBoolean("micOn", false)
    var autostart: Boolean = prefs.getBoolean("autostart", true)
    var restore: Boolean = prefs.getBoolean("restore", true)
    var background: Boolean = prefs.getBoolean("background", true)
    var virtualMic: Boolean = false
    var listen: MutableList<SavedListen> = mutableListOf()

    init {
        if (id.isBlank()) id = UUID.randomUUID().toString().replace("-", "")
        if (alias.isBlank()) alias = defaultAlias()
        listen = mutableListOf()
        val raw = prefs.getString("listen", null)
        if (!raw.isNullOrBlank()) {
            try {
                val array = JSONArray(raw)
                for (index in 0 until array.length()) {
                    array.optJSONObject(index)?.let { listen.add(SavedListen.from(it)) }
                }
            } catch (_: Exception) {
                listen.clear()
            }
        }
        save()
    }

    fun save() {
        val array = JSONArray()
        listen.forEach { array.put(it.json()) }
        prefs.edit()
            .putString("id", id)
            .putString("alias", alias)
            .putString("outputId", outputId)
            .putString("micId", micId)
            .putBoolean("systemOn", systemOn)
            .putBoolean("micOn", micOn)
            .putBoolean("autostart", autostart)
            .putBoolean("restore", restore)
            .putBoolean("background", background)
            .putString("listen", array.toString())
            .commit()
    }

    fun setBroadcast(kind: SourceKind, on: Boolean) {
        when (kind) {
            SourceKind.SYSTEM -> systemOn = on
            SourceKind.MICROPHONE -> micOn = on
        }
        save()
    }

    fun rememberListen(peerId: String, kind: SourceKind, alias: String, source: String) {
        val saved = SavedListen(peerId, kind.label, alias, source)
        val index = listen.indexOfFirst { it.peerId == peerId && it.kind == kind.label }
        when {
            index < 0 -> listen.add(saved)
            listen[index].same(saved) -> return
            else -> listen[index] = saved
        }
        save()
    }

    fun forgetListen(key: String) {
        val (peerId, kind) = parseKey(key) ?: return
        val before = listen.size
        listen.removeAll { it.peerId == peerId && it.kind == kind.label }
        if (listen.size != before) save()
    }

    private fun defaultAlias(): String {
        val model = Build.MODEL.orEmpty().trim()
        val fallback = if (model.isEmpty()) "locoSaonduh" else model
        return fallback.replace('-', ' ').replace('_', ' ').take(32).trim()
    }
}
