import 'package:pigeon/pigeon.dart';

@ConfigurePigeon(
  PigeonOptions(
    dartOut: 'lib/src/windows_webview_api.g.dart',
    cppOptions: CppOptions(namespace: 'webview_all_windows'),
    cppHeaderOut: 'windows/generated/windows_webview_api.g.h',
    cppSourceOut: 'windows/generated/windows_webview_api.g.cpp',
  ),
)
class WindowsEnvironmentOptions {
  WindowsEnvironmentOptions({
    this.userDataPath,
    this.browserExePath,
    this.additionalArguments,
  });

  String? userDataPath;
  String? browserExePath;
  String? additionalArguments;
}

class WindowsCreateWebViewResult {
  WindowsCreateWebViewResult({required this.viewId});

  int viewId;
}

class WindowsCookieData {
  WindowsCookieData({
    required this.name,
    required this.value,
    required this.domain,
    required this.path,
    this.expires,
    this.isHttpOnly,
    this.isSecure,
    this.sameSite,
    this.isSession,
  });

  String name;
  String value;
  String domain;
  String path;
  double? expires;
  bool? isHttpOnly;
  bool? isSecure;
  int? sameSite;
  bool? isSession;
}

class WindowsPointData {
  WindowsPointData({required this.x, required this.y});

  double x;
  double y;
}

class WindowsSizeData {
  WindowsSizeData({
    required this.width,
    required this.height,
    required this.scaleFactor,
  });

  double width;
  double height;
  double scaleFactor;
}

class WindowsPointerUpdateData {
  WindowsPointerUpdateData({
    required this.pointer,
    required this.event,
    required this.x,
    required this.y,
    required this.size,
    required this.pressure,
  });

  int pointer;
  int event;
  double x;
  double y;
  double size;
  double pressure;
}

class WindowsPointerButtonData {
  WindowsPointerButtonData({required this.button, required this.isDown});

  int button;
  bool isDown;
}

class WindowsVirtualHostMappingData {
  WindowsVirtualHostMappingData({
    required this.hostName,
    required this.path,
    required this.accessKind,
  });

  String hostName;
  String path;
  int accessKind;
}

class WindowsLoadRequestData {
  WindowsLoadRequestData({
    required this.url,
    required this.method,
    required this.headers,
    this.body,
  });

  String url;
  String method;
  String headers;
  Uint8List? body;
}

@HostApi()
abstract class WindowsWebViewHostApi {
  @async
  void initializeEnvironment(WindowsEnvironmentOptions options);

  String? getWebViewVersion();

  @async
  WindowsCreateWebViewResult createWebView();

  void disposeWebView(int viewId);

  void loadUrl(int viewId, String url);

  void loadRequest(int viewId, WindowsLoadRequestData request);

  void loadStringContent(int viewId, String content);

  void reload(int viewId);

  void stop(int viewId);

  void goBack(int viewId);

  void goForward(int viewId);

  @async
  String? addScriptToExecuteOnDocumentCreated(int viewId, String script);

  void removeScriptToExecuteOnDocumentCreated(int viewId, String scriptId);

  @async
  String executeScript(int viewId, String script);

  void postWebMessage(int viewId, String message);

  void setUserAgent(int viewId, String? userAgent);

  String? getUserAgent(int viewId);

  void setJavaScriptEnabled(int viewId, bool enabled);

  @async
  bool clearCookies(int viewId);

  void setCookie(int viewId, WindowsCookieData cookie);

  @async
  List<WindowsCookieData?> getCookies(int viewId, String url);

  void deleteCookie(int viewId, WindowsCookieData cookie);

  void deleteCookiesWithNameAndUrl(int viewId, String name, String url);

  void deleteCookiesWithNameDomainAndPath(
    int viewId,
    String name,
    String domain,
    String path,
  );

  void clearCache(int viewId);

  @async
  void clearLocalStorage(int viewId);

  void setCacheDisabled(int viewId, bool disabled);

  void openDevTools(int viewId);

  void setBackgroundColor(int viewId, int color);

  void setZoomControlEnabled(int viewId, bool enabled);

  void setZoomFactor(int viewId, double zoomFactor);

  void setPopupWindowPolicy(int viewId, int policy);

  void setJavaScriptDialogCallbacksEnabled(
    int viewId,
    bool alert,
    bool confirm,
    bool prompt,
  );

  void suspend(int viewId);

  void resume(int viewId);

  void setVirtualHostNameMapping(
    int viewId,
    WindowsVirtualHostMappingData mapping,
  );

  void clearVirtualHostNameMapping(int viewId, String hostName);

  void setFpsLimit(int viewId, int maxFps);

  void setPointerUpdate(int viewId, WindowsPointerUpdateData update);

  void setCursorPos(int viewId, WindowsPointData position);

  void setPointerButton(int viewId, WindowsPointerButtonData button);

  void setScrollDelta(
    int viewId,
    WindowsPointData delta,
    bool controlKeyPressed,
  );

  void setSize(int viewId, WindowsSizeData size);
}
