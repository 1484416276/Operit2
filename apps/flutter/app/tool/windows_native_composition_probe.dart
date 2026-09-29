import 'dart:async';
import 'dart:convert';
import 'dart:developer' as developer;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:webview_all_windows/src/windows_webview_native.dart' as browser;

/// Starts a deterministic Windows native-composition acceptance fixture.
void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const MaterialApp(home: NativeCompositionProbe()));
}

class NativeCompositionProbe extends StatefulWidget {
  /// Creates the browser, scrolling and modal-overlay test surface.
  const NativeCompositionProbe({super.key});

  /// Creates the fixture controller state.
  @override
  State<NativeCompositionProbe> createState() => _NativeCompositionProbeState();
}

class _NativeCompositionProbeState extends State<NativeCompositionProbe> {
  final browser.WebviewController controller = browser.WebviewController();
  String status = 'Initializing native browser';
  bool ready = false;

  /// Initializes WebView2 after the Flutter view has been created.
  @override
  void initState() {
    super.initState();
    developer.registerExtension('ext.operit.probe', (method, parameters) async {
      switch (parameters['action']) {
        case 'dialog':
          unawaited(openDialog());
        case 'close':
          Navigator.of(context).pop();
        case 'tap':
          final position = Offset(
            double.parse(parameters['x']!),
            double.parse(parameters['y']!),
          );
          GestureBinding.instance.handlePointerEvent(
            PointerDownEvent(pointer: 77, position: position),
          );
          GestureBinding.instance.handlePointerEvent(
            PointerUpEvent(pointer: 77, position: position),
          );
        case 'script':
          final result = await controller.executeScript(parameters['script']!);
          return developer.ServiceExtensionResponse.result(
            jsonEncode(<String, Object?>{'result': result}),
          );
      }
      if (!mounted) {
        return developer.ServiceExtensionResponse.error(
          developer.ServiceExtensionResponse.extensionError,
          'The acceptance surface has been disposed.',
        );
      }
      final view = View.of(context);
      return developer.ServiceExtensionResponse.result(
        jsonEncode(<String, Object?>{
          'ready': ready,
          'status': status,
          'width': view.physicalSize.width,
          'height': view.physicalSize.height,
          'scale': view.devicePixelRatio,
        }),
      );
    });
    unawaited(initialize());
  }

  /// Loads local deterministic HTML with visible orientation and input counters.
  Future<void> initialize() async {
    try {
      await controller.initialize();
      await controller.loadStringContent(
        '''<!doctype html><html><body style="margin:0;background:#00ff00;font:24px sans-serif">
<div style="height:200px">TOP GREEN — native WebView2 <input placeholder="Test IME and keyboard"></div>
<button onclick="this.textContent='Browser clicks: '+(++window.clicks)">Browser clicks: 0</button>
<div style="height:1200px;background:linear-gradient(#00ff00,#ff0000)">Scroll the native browser</div>
<div style="background:red">BOTTOM RED</div><script>window.clicks=0;</script></body></html>''',
      );
      if (!mounted) return;
      setState(() {
        ready = true;
        status = 'Native browser ready';
      });
    } catch (error) {
      if (mounted) setState(() => status = error.toString());
    }
  }

  /// Displays a standard Flutter dialog and a translucent full-window barrier.
  Future<void> openDialog() async {
    await showDialog<void>(
      context: context,
      barrierColor: const Color(0x80000000),
      builder: (context) => AlertDialog(
        title: const Text('Flutter ABOVE native WebView2'),
        content: const SizedBox(
          width: 400,
          child: TextField(
            decoration: InputDecoration(
              labelText: 'Dialog input must own keyboard',
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close dialog'),
          ),
        ],
      ),
    );
  }

  /// Releases the browser when the fixture is unmounted.
  @override
  void dispose() {
    unawaited(controller.dispose());
    super.dispose();
  }

  /// Paints Flutter controls around the native layer and exposes the modal test.
  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: Text(status),
      actions: [
        TextButton(
          onPressed: ready ? openDialog : null,
          child: const Text('OPEN FLUTTER DIALOG'),
        ),
      ],
    ),
    body: ready
        ? Padding(
            padding: const EdgeInsets.all(32),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(24),
              child: browser.Webview(controller),
            ),
          )
        : const Center(child: CircularProgressIndicator()),
  );
}
