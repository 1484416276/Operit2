package io.flutter.plugins.webviewflutter;

import android.os.Handler;
import android.os.Looper;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import io.flutter.plugin.common.BinaryMessenger;
import io.flutter.plugin.common.MethodChannel;
import java.io.ByteArrayInputStream;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.WeakHashMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/** Native virtual-HTTPS loader. Only the WebView IO thread waits for Dart. */
final class WebViewLocalResources {
  private final MethodChannel channel;
  private final ProxyApiRegistrar registrar;
  private final Map<WebView, Long> enabled = Collections.synchronizedMap(new WeakHashMap<>());
  private final Handler main = new Handler(Looper.getMainLooper());

  WebViewLocalResources(BinaryMessenger messenger, ProxyApiRegistrar registrar) {
    this.registrar = registrar;
    channel = new MethodChannel(messenger, "operit/webview_resources");
    channel.setMethodCallHandler((call, result) -> {
      if (!call.method.equals("configure")) { result.notImplemented(); return; }
      Number id = call.argument("identifier");
      Boolean active = call.argument("enabled");
      if (id == null || active == null) { result.error("invalid_arguments", "Missing view identifier", null); return; }
      WebView view = registrar.getInstanceManager().getInstance(id.longValue());
      if (view == null) { result.error("unknown_view", "WebView no longer exists", null); return; }
      if (active) enabled.put(view, id.longValue()); else enabled.remove(view);
      result.success(null);
    });
  }

  WebResourceResponse intercept(WebView view, WebResourceRequest request) {
    String host = request.getUrl().getHost();
    if (!"https".equals(request.getUrl().getScheme()) || host == null || !host.endsWith(".operit.invalid")) return null;
    Long id = enabled.get(view);
    if (id == null) return failure(403, "Forbidden");
    if (Looper.myLooper() == Looper.getMainLooper()) return failure(500, "Invalid thread");
    CountDownLatch ready = new CountDownLatch(1);
    AtomicReference<Object> response = new AtomicReference<>();
    Map<String, Object> args = new HashMap<>();
    args.put("identifier", id); args.put("url", request.getUrl().toString());
    args.put("method", request.getMethod()); args.put("headers", request.getRequestHeaders());
    args.put("isMainFrame", request.isForMainFrame());
    main.post(() -> channel.invokeMethod("request", args, new MethodChannel.Result() {
      public void success(Object value) { response.set(value); ready.countDown(); }
      public void error(String code, String message, Object details) { ready.countDown(); }
      public void notImplemented() { ready.countDown(); }
    }));
    try {
      if (!ready.await(35, TimeUnit.SECONDS)) return failure(504, "Resource timeout");
      if (!(response.get() instanceof Map)) return failure(500, "Resource error");
      Map<?, ?> value = (Map<?, ?>) response.get();
      @SuppressWarnings("unchecked")
      Map<String, String> headers = (Map<String, String>) value.get("headers");
      return new WebResourceResponse((String) value.get("mimeType"), (String) value.get("encoding"),
          ((Number) value.get("statusCode")).intValue(), (String) value.get("reasonPhrase"),
          headers, new ByteArrayInputStream((byte[]) value.get("body")));
    } catch (InterruptedException error) {
      Thread.currentThread().interrupt(); return failure(503, "Interrupted");
    } catch (RuntimeException error) { return failure(500, "Invalid resource response"); }
  }

  private WebResourceResponse failure(int status, String reason) {
    return new WebResourceResponse("text/plain", "utf-8", status, reason,
        Collections.emptyMap(), new ByteArrayInputStream(new byte[0]));
  }

  void dispose() { channel.setMethodCallHandler(null); enabled.clear(); }
}
