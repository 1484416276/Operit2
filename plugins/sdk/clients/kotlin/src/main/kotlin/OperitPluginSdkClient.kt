package operit.plugin.sdk

import com.fasterxml.jackson.databind.ObjectMapper
import org.msgpack.jackson.dataformat.MessagePackFactory
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/** One Core watch event returned through Plugin SDK IPC. */
data class OperitPluginSdkEvent(
    val requestId: String?,
    val target: String,
    val propertyName: String,
    val kind: String,
    val value: Any?,
)

/** One caller-owned Core push stream. */
interface OperitPluginSdkPushSink {
    /** Sends one ordered Link value to Core. */
    suspend fun add(value: Any?)
    /** Closes the Core push stream. */
    suspend fun close()
}

/** Concrete JVM Plugin SDK client using the standard framed Link IPC carrier. */
class OperitPluginSdkClient private constructor(private val handle: Long) : AutoCloseable {
    private val mapper = ObjectMapper(MessagePackFactory())
    private val closed = AtomicBoolean(false)
    private val nextId = AtomicLong(1)
    private val pending = ConcurrentHashMap<String, (Result<Any?>) -> Unit>()
    private val watches = ConcurrentHashMap<String, Channel<OperitPluginSdkEvent>>()

    init {
        Thread(::readLoop, "operit-plugin-sdk-ipc").apply { isDaemon = true }.start()
    }

    /** Calls one generated Core method. */
    suspend fun call(target: String, methodName: String, args: Any?): Any? {
        val id = requestId()
        return request(id, mapOf("type" to "Call", "body" to mapOf("requestId" to id, "target" to target, "methodName" to methodName, "args" to (args ?: emptyMap<String, Any?>()))))
    }

    /** Calls one route and decodes its MessagePack value. */
    suspend fun <T> callTyped(target: String, methodName: String, args: Any?, decode: (Any?) -> T): T = decode(call(target, methodName, args))

    /** Watches one generated Core property. */
    fun watch(target: String, propertyName: String, args: Any?): Flow<OperitPluginSdkEvent> = flow {
        val id = requestId()
        val channel = Channel<OperitPluginSdkEvent>(Channel.BUFFERED)
        watches[id] = channel
        try {
            request(id, mapOf("type" to "WatchOpen", "body" to mapOf("subscriptionId" to id, "request" to mapOf("requestId" to id, "target" to target, "propertyName" to propertyName, "args" to (args ?: emptyMap<String, Any?>())))))
            for (event in channel) emit(event)
        } finally {
            watches.remove(id)
            if (!closed.get()) send(mapOf("type" to "WatchClose", "body" to mapOf("subscriptionId" to id, "error" to null)))
        }
    }

    /** Watches one route and decodes each MessagePack event value. */
    fun <T> watchTyped(target: String, propertyName: String, args: Any?, decode: (Any?) -> T): Flow<T> = flow {
        watch(target, propertyName, args).collect { emit(decode(it.value)) }
    }

    /** Opens one generated caller-owned Core input stream. */
    suspend fun push(target: String, methodName: String, args: Any?): OperitPluginSdkPushSink {
        val id = requestId()
        request(id, mapOf("type" to "PushOpen", "body" to mapOf("requestId" to id, "target" to target, "methodName" to methodName, "args" to (args ?: emptyMap<String, Any?>()))))
        return object : OperitPluginSdkPushSink {
            private var sequence = 0L
            /** Sends the next ordered item and waits for its acknowledgement. */
            override suspend fun add(value: Any?) { request("$id:$sequence", mapOf("type" to "PushItem", "body" to mapOf("pushId" to id, "sequence" to sequence++, "args" to value))) }
            /** Closes the input stream and waits for Core to acknowledge it. */
            override suspend fun close() { request(id, mapOf("type" to "PushClose", "body" to mapOf("pushId" to id))) }
        }
    }

    companion object {
        /** Activates Operit through the SDK's platform host and waits for its session. */
        suspend fun connect(): OperitPluginSdkClient = withContext(Dispatchers.IO) {
            OperitPluginSdkClient(OperitPluginSdkHost.connect())
        }
    }

    /** Closes the native carrier and fails every operation still awaiting a response. */
    override fun close() {
        if (closed.compareAndSet(false, true)) {
            try { OperitPluginSdkHost.close(handle) }
            finally { fail(IllegalStateException("Plugin SDK connection closed")) }
        }
    }

    /** Completes outstanding calls and watch streams with a transport error. */
    private fun fail(error: Throwable) {
        pending.keys.toList().forEach { pending.remove(it)?.invoke(Result.failure(error)) }
        watches.keys.toList().forEach { watches.remove(it)?.close(error) }
    }

    /** Allocates one session-local request id. */
    private fun requestId(): String = "plugin-sdk-${nextId.getAndIncrement()}"

    /** Registers one request and sends its MessagePack envelope. */
    private suspend fun request(id: String, message: Any?): Any? = suspendCancellableCoroutine { continuation ->
        pending[id] = { result -> continuation.resumeWith(result) }
        continuation.invokeOnCancellation { pending.remove(id) }
        try { send(message) } catch (error: Exception) { pending.remove(id)?.invoke(Result.failure(error)) }
    }

    /** Sends an envelope through the host-owned framing and IPC implementation. */
    private fun send(message: Any?) {
        val bytes = mapper.writeValueAsBytes(message)
        check(!closed.get()) { "Plugin SDK connection closed" }
        OperitPluginSdkHost.send(handle, bytes)
    }

    /** Reads and dispatches framed MessagePack messages. */
    private fun readLoop() {
        try {
            while (!closed.get()) {
                val bytes = OperitPluginSdkHost.next(handle) ?: continue
                dispatch(mapper.readValue(bytes, Map::class.java))
            }
        } catch (error: Exception) {
            fail(error)
            if (closed.compareAndSet(false, true)) {
                try { OperitPluginSdkHost.close(handle) } catch (closeError: Exception) { error.addSuppressed(closeError) }
            }
        }
    }

    /** Routes one decoded protocol envelope. */
    @Suppress("UNCHECKED_CAST")
    private fun dispatch(message: Map<*, *>) {
        val type = message["type"]
        val body = message["body"] as Map<*, *>
        when (type) {
            "CallResponse" -> complete(body["requestId"].toString(), body["result"])
            "WatchOpened" -> complete(body["subscriptionId"].toString(), body["result"])
            "PushOpened", "PushClosed" -> complete(body["pushId"].toString(), body["result"])
            "PushItemResult" -> complete("${body["pushId"]}:${body["sequence"]}", body["result"])
            "WatchEvent" -> {
                val event = body["event"] as Map<*, *>
                watches[body["subscriptionId"].toString()]?.trySend(OperitPluginSdkEvent(event["requestId"] as String?, event["target"] as String, event["propertyName"].toString(), event["kind"].toString(), event["value"]))
            }
            "WatchClose" -> watches.remove(body["subscriptionId"].toString())?.close()
        }
    }

    /** Completes one pending request with its Result payload. */
    private fun complete(id: String, result: Any?) {
        val response = result as Map<*, *>
        val decoded = when (response.keys.single()) {
            "Ok" -> Result.success(response["Ok"])
            "Err" -> Result.failure<Any?>(IllegalStateException(response["Err"].toString()))
            else -> throw IllegalStateException("Invalid Core Link result")
        }
        pending.remove(id)?.invoke(decoded)
    }
}
