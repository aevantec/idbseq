// idbseq from Java 17+, JDK only (java.net.http). Run: java IdbSeqExample.java
//
// Environment:
//   IDBSEQ_URL         server base URL (default http://127.0.0.1:7878)
//   IDBSEQ_KEY         API key, required; needs the `read` and `generate` scopes
//   IDBSEQ_COLLECTION  an existing collection (default `examples`)
//
// Runs: generate one ID, generate a batch of 3, read the last ID, list
// collections (following meta.next_cursor). Exits non-zero on any failure.
// The JDK has no JSON parser, so a minimal one is included below; it reads
// integers as long, which holds every idbseq ID. In a real project use
// Jackson or Gson and map IDs to long.

import java.io.IOException;
import java.net.URI;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

public class IdbSeqExample {
    static final String DEFAULT_URL = "http://127.0.0.1:7878";
    static final Duration TIMEOUT = Duration.ofSeconds(10);
    static final int PAGE_LIMIT = 100;

    /** A failed call: HTTP status (0 for network errors), error code, message. */
    static final class ApiException extends Exception {
        final int status;
        final String code;

        ApiException(int status, String code, String message) {
            super(code + " (HTTP " + status + "): " + message);
            this.status = status;
            this.code = code;
        }
    }

    private final HttpClient http = HttpClient.newBuilder().connectTimeout(TIMEOUT).build();
    private final String baseUrl;
    private final String key;

    IdbSeqExample(String baseUrl, String key) {
        this.baseUrl = baseUrl.replaceAll("/+$", "");
        this.key = key;
    }

    /** Sends a request and returns the success envelope, or throws ApiException. */
    Map<String, Object> call(String method, String pathAndQuery, Map<String, String> headers)
            throws ApiException {
        HttpRequest.Builder builder = HttpRequest.newBuilder(URI.create(baseUrl + pathAndQuery))
                .timeout(TIMEOUT)
                .header("Authorization", "Bearer " + key)
                .header("Accept", "application/json")
                .method(method, HttpRequest.BodyPublishers.noBody());
        headers.forEach(builder::header);

        HttpResponse<String> response;
        try {
            response = http.send(builder.build(), HttpResponse.BodyHandlers.ofString());
        } catch (IOException e) {
            throw new ApiException(0, "NETWORK_ERROR", e.toString());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new ApiException(0, "INTERRUPTED", e.toString());
        }

        int status = response.statusCode();
        Map<String, Object> envelope;
        try {
            envelope = asMap(Json.parse(response.body()));
        } catch (IllegalArgumentException e) {
            envelope = null;
        }
        if (envelope == null) {
            throw new ApiException(status, "HTTP_" + status, response.body());
        }
        if (status / 100 == 2 && Boolean.TRUE.equals(envelope.get("success"))) {
            return envelope;
        }
        // Unknown or missing codes are still errors of this HTTP status.
        Map<String, Object> error = asMap(envelope.get("error"));
        String code = error != null && error.get("code") instanceof String c ? c : "HTTP_" + status;
        String message = error != null && error.get("message") instanceof String m ? m : "";
        throw new ApiException(status, code, message);
    }

    Map<String, Object> data(String method, String pathAndQuery, Map<String, String> headers)
            throws ApiException {
        Map<String, Object> data = asMap(call(method, pathAndQuery, headers).get("data"));
        if (data == null) {
            throw new ApiException(200, "DECODE_ERROR", "expected an object in `data`");
        }
        return data;
    }

    static String collectionPath(String name, String suffix) {
        return "/v1/collections/" + encode(name) + suffix;
    }

    long generate(String name) throws ApiException {
        return (Long) data("POST", collectionPath(name, "/next"), Map.of()).get("id");
    }

    Map<String, Object> generateBatch(String name, int count, String idempotencyKey) throws ApiException {
        String path = collectionPath(name, "/next") + "?count=" + count;
        return data("POST", path, Map.of("Idempotency-Key", idempotencyKey));
    }

    Map<String, Object> last(String name) throws ApiException {
        return data("GET", collectionPath(name, "/last"), Map.of());
    }

    /** Returns every collection, following meta.next_cursor. */
    List<Map<String, Object>> listAll() throws ApiException {
        List<Map<String, Object>> all = new ArrayList<>();
        String cursor = null;
        do {
            String query = "?limit=" + PAGE_LIMIT + (cursor == null ? "" : "&cursor=" + encode(cursor));
            Map<String, Object> envelope = call("GET", "/v1/collections" + query, Map.of());
            if (!(envelope.get("data") instanceof List<?> page)) {
                throw new ApiException(200, "DECODE_ERROR", "expected an array in `data`");
            }
            for (Object item : page) {
                all.add(asMap(item));
            }
            Map<String, Object> meta = asMap(envelope.get("meta"));
            cursor = meta != null && meta.get("next_cursor") instanceof String next ? next : null;
        } while (cursor != null);
        return all;
    }

    static String encode(String value) {
        return URLEncoder.encode(value, StandardCharsets.UTF_8);
    }

    @SuppressWarnings("unchecked")
    static Map<String, Object> asMap(Object value) {
        return value instanceof Map<?, ?> ? (Map<String, Object>) value : null;
    }

    static String envOr(String name, String fallback) {
        String value = System.getenv(name);
        return value == null || value.isEmpty() ? fallback : value;
    }

    void run(String collection) throws ApiException {
        System.out.println("next:  id=" + generate(collection));

        Map<String, Object> batch = generateBatch(collection, 3, "example-" + UUID.randomUUID());
        System.out.printf("batch: first=%s last=%s count=%s step=%s%n",
                batch.get("first"), batch.get("last"), batch.get("count"), batch.get("step"));

        Map<String, Object> last = last(collection);
        System.out.printf("last:  last_id=%s next_id=%s%n", last.get("last_id"), last.get("next_id"));

        for (Map<String, Object> item : listAll()) {
            System.out.printf("list:  %s last_id=%s next_id=%s%n",
                    item.get("name"), item.get("last_id"), item.get("next_id"));
        }
    }

    public static void main(String[] args) {
        String key = System.getenv("IDBSEQ_KEY");
        if (key == null || key.isEmpty()) {
            System.err.println("error: IDBSEQ_KEY is not set (an API key with read + generate scopes)");
            System.exit(2);
        }
        IdbSeqExample client = new IdbSeqExample(envOr("IDBSEQ_URL", DEFAULT_URL), key);
        try {
            client.run(envOr("IDBSEQ_COLLECTION", "examples"));
        } catch (ApiException e) {
            System.err.println("error: " + e.getMessage());
            System.exit(1);
        }
    }

    /** Minimal JSON reader: objects, arrays, strings, long/double, booleans, null. */
    static final class Json {
        private final String text;
        private int pos;

        private Json(String text) {
            this.text = text;
        }

        static Object parse(String text) {
            Json parser = new Json(text);
            Object value = parser.value();
            parser.skipWhitespace();
            if (parser.pos != text.length()) {
                throw parser.fail("trailing data");
            }
            return value;
        }

        private IllegalArgumentException fail(String what) {
            return new IllegalArgumentException("invalid JSON: " + what + " at offset " + pos);
        }

        private void skipWhitespace() {
            while (pos < text.length() && " \t\r\n".indexOf(text.charAt(pos)) >= 0) {
                pos++;
            }
        }

        private boolean peek(char c) {
            return pos < text.length() && text.charAt(pos) == c;
        }

        private void expect(char c) {
            skipWhitespace();
            if (!peek(c)) {
                throw fail("expected '" + c + "'");
            }
            pos++;
        }

        private Object value() {
            skipWhitespace();
            if (pos >= text.length()) {
                throw fail("unexpected end");
            }
            return switch (text.charAt(pos)) {
                case '{' -> object();
                case '[' -> array();
                case '"' -> string();
                case 't' -> literal("true", Boolean.TRUE);
                case 'f' -> literal("false", Boolean.FALSE);
                case 'n' -> literal("null", null);
                default -> number();
            };
        }

        private Object literal(String word, Object result) {
            if (!text.startsWith(word, pos)) {
                throw fail("unexpected token");
            }
            pos += word.length();
            return result;
        }

        private Map<String, Object> object() {
            Map<String, Object> result = new LinkedHashMap<>();
            expect('{');
            skipWhitespace();
            if (peek('}')) {
                pos++;
                return result;
            }
            while (true) {
                skipWhitespace();
                if (!peek('"')) {
                    throw fail("expected a key");
                }
                String name = string();
                expect(':');
                result.put(name, value());
                skipWhitespace();
                if (peek(',')) {
                    pos++;
                    continue;
                }
                expect('}');
                return result;
            }
        }

        private List<Object> array() {
            List<Object> result = new ArrayList<>();
            expect('[');
            skipWhitespace();
            if (peek(']')) {
                pos++;
                return result;
            }
            while (true) {
                result.add(value());
                skipWhitespace();
                if (peek(',')) {
                    pos++;
                    continue;
                }
                expect(']');
                return result;
            }
        }

        private String string() {
            StringBuilder out = new StringBuilder();
            pos++; // opening quote
            while (true) {
                if (pos >= text.length()) {
                    throw fail("unterminated string");
                }
                char c = text.charAt(pos++);
                if (c == '"') {
                    return out.toString();
                }
                if (c != '\\') {
                    out.append(c);
                    continue;
                }
                if (pos >= text.length()) {
                    throw fail("unterminated escape");
                }
                char escape = text.charAt(pos++);
                switch (escape) {
                    case '"', '\\', '/' -> out.append(escape);
                    case 'b' -> out.append('\b');
                    case 'f' -> out.append('\f');
                    case 'n' -> out.append('\n');
                    case 'r' -> out.append('\r');
                    case 't' -> out.append('\t');
                    case 'u' -> {
                        if (pos + 4 > text.length()) {
                            throw fail("bad unicode escape");
                        }
                        out.append((char) Integer.parseInt(text.substring(pos, pos + 4), 16));
                        pos += 4;
                    }
                    default -> throw fail("bad escape");
                }
            }
        }

        private Object number() {
            int start = pos;
            while (pos < text.length() && "+-0123456789.eE".indexOf(text.charAt(pos)) >= 0) {
                pos++;
            }
            String token = text.substring(start, pos);
            if (token.isEmpty()) {
                throw fail("unexpected character");
            }
            boolean isInteger = token.indexOf('.') < 0 && token.indexOf('e') < 0 && token.indexOf('E') < 0;
            return isInteger ? (Object) Long.parseLong(token) : (Object) Double.parseDouble(token);
        }
    }
}
