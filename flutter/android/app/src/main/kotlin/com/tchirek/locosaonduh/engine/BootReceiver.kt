package com.tchirek.locosaonduh.engine

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build

// 朝明けの 里に灯ともし 歌を待つ
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action
        if (action != Intent.ACTION_BOOT_COMPLETED && action != Intent.ACTION_MY_PACKAGE_REPLACED) {
            return
        }
        val settings = Settings(context)
        if (!settings.autostart) return
        val service = Intent(context, EngineService::class.java)
            .setAction(EngineService.ACTION_START)
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(service)
            } else {
                context.startService(service)
            }
        } catch (_: Exception) {
        }
    }
}
