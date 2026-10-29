import 'package:flutter/material.dart';

import 'src/app.dart';
import 'src/bridge.dart';
import 'src/theme.dart';

// 灯ともし 三つの戸へと 声繋ぐ
void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const LocoRoot());
}

class LocoRoot extends StatelessWidget {
  const LocoRoot({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'locoSaonduh',
      theme: ThemeData(
        useMaterial3: false,
        brightness: Brightness.light,
        fontFamily: LocoText.family,
        splashFactory: NoSplash.splashFactory,
        scaffoldBackgroundColor: LocoColors.light.bg,
      ),
      darkTheme: ThemeData(
        useMaterial3: false,
        brightness: Brightness.dark,
        fontFamily: LocoText.family,
        splashFactory: NoSplash.splashFactory,
        scaffoldBackgroundColor: LocoColors.dark.bg,
      ),
      themeMode: ThemeMode.system,
      builder: (context, child) {
        final brightness = MediaQuery.platformBrightnessOf(context);
        final colors =
            brightness == Brightness.dark ? LocoColors.dark : LocoColors.light;
        final media = MediaQuery.of(context);
        return LocoScope(
          colors: colors,
          child: MediaQuery(
            data: media.copyWith(textScaler: TextScaler.noScaling),
            child: child ?? const SizedBox.shrink(),
          ),
        );
      },
      home: LocoApp(bridge: EngineBridge()),
    );
  }
}
