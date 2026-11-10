// idbseq from Go, standard library only (Go 1.22+).
//
// Environment:
//
//	IDBSEQ_URL         server base URL (default http://127.0.0.1:7878)
//	IDBSEQ_KEY         API key, required; needs the `read` and `generate` scopes
//	IDBSEQ_COLLECTION  an existing collection (default `examples`)
//
// Runs: generate one ID, generate a batch of 3, read the last ID, list
// collections (following meta.next_cursor). Exits non-zero on any failure.
// IDs are decoded into int64 fields, which is lossless.
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	defaultURL = "http://127.0.0.1:7878"
	timeout    = 10 * time.Second
	pageLimit  = 100
)

type apiError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

type meta struct {
	Total      int64   `json:"total"`
	Limit      int64   `json:"limit"`
	NextCursor *string `json:"next_cursor"`
}

type envelope struct {
	Success bool            `json:"success"`
	Data    json.RawMessage `json:"data"`
	Error   *apiError       `json:"error"`
	Meta    *meta           `json:"meta"`
}

// requestError is a failed call: HTTP status (0 for network errors), code, message.
type requestError struct {
	Status  int
	Code    string
	Message string
}

func (e *requestError) Error() string {
	return fmt.Sprintf("%s (HTTP %d): %s", e.Code, e.Status, e.Message)
}

type nextID struct {
	ID int64 `json:"id"`
}

type batch struct {
	First int64 `json:"first"`
	Last  int64 `json:"last"`
	Count int64 `json:"count"`
	Step  int64 `json:"step"`
}

type lastID struct {
	LastID *int64 `json:"last_id"`
	NextID *int64 `json:"next_id"`
}

type collection struct {
	Name   string `json:"name"`
	LastID *int64 `json:"last_id"`
	NextID *int64 `json:"next_id"`
}

type client struct {
	baseURL    string
	key        string
	httpClient *http.Client
}

// call sends a request and returns the success envelope, or a *requestError.
func (c *client) call(method, path string, query url.Values, headers map[string]string) (*envelope, error) {
	target := c.baseURL + path
	if len(query) > 0 {
		target += "?" + query.Encode()
	}
	req, err := http.NewRequest(method, target, nil)
	if err != nil {
		return nil, &requestError{Code: "BAD_REQUEST", Message: err.Error()}
	}
	req.Header.Set("Authorization", "Bearer "+c.key)
	req.Header.Set("Accept", "application/json")
	for name, value := range headers {
		req.Header.Set(name, value)
	}
	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, &requestError{Code: "NETWORK_ERROR", Message: err.Error()}
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, &requestError{Status: resp.StatusCode, Code: "NETWORK_ERROR", Message: err.Error()}
	}

	var env envelope
	decodeErr := json.Unmarshal(raw, &env)
	if decodeErr == nil && env.Success && resp.StatusCode/100 == 2 {
		return &env, nil
	}
	// Unknown or missing codes are still errors of this HTTP status.
	failure := &requestError{Status: resp.StatusCode, Code: "HTTP_" + strconv.Itoa(resp.StatusCode), Message: string(raw)}
	if decodeErr == nil && env.Error != nil {
		if env.Error.Code != "" {
			failure.Code = env.Error.Code
		}
		failure.Message = env.Error.Message
	}
	return nil, failure
}

// data calls the API and decodes the envelope's data into out.
func (c *client) data(method, path string, query url.Values, headers map[string]string, out any) error {
	env, err := c.call(method, path, query, headers)
	if err != nil {
		return err
	}
	if err := json.Unmarshal(env.Data, out); err != nil {
		return &requestError{Status: 200, Code: "DECODE_ERROR", Message: err.Error()}
	}
	return nil
}

func collectionPath(name, suffix string) string {
	return "/v1/collections/" + url.PathEscape(name) + suffix
}

func (c *client) generate(name string) (int64, error) {
	var out nextID
	err := c.data(http.MethodPost, collectionPath(name, "/next"), nil, nil, &out)
	return out.ID, err
}

func (c *client) generateBatch(name string, count int, idempotencyKey string) (batch, error) {
	var out batch
	query := url.Values{"count": {strconv.Itoa(count)}}
	headers := map[string]string{"Idempotency-Key": idempotencyKey}
	err := c.data(http.MethodPost, collectionPath(name, "/next"), query, headers, &out)
	return out, err
}

func (c *client) last(name string) (lastID, error) {
	var out lastID
	err := c.data(http.MethodGet, collectionPath(name, "/last"), nil, nil, &out)
	return out, err
}

// listAll returns every collection, following meta.next_cursor.
func (c *client) listAll() ([]collection, error) {
	var all []collection
	cursor := ""
	for {
		query := url.Values{"limit": {strconv.Itoa(pageLimit)}}
		if cursor != "" {
			query.Set("cursor", cursor)
		}
		env, err := c.call(http.MethodGet, "/v1/collections", query, nil)
		if err != nil {
			return nil, err
		}
		var page []collection
		if err := json.Unmarshal(env.Data, &page); err != nil {
			return nil, &requestError{Status: 200, Code: "DECODE_ERROR", Message: err.Error()}
		}
		all = append(all, page...)
		if env.Meta == nil || env.Meta.NextCursor == nil {
			return all, nil
		}
		cursor = *env.Meta.NextCursor
	}
}

func optional(v *int64) string {
	if v == nil {
		return "null"
	}
	return strconv.FormatInt(*v, 10)
}

func envOr(name, fallback string) string {
	if v := os.Getenv(name); v != "" {
		return v
	}
	return fallback
}

func run(c *client, name string) error {
	id, err := c.generate(name)
	if err != nil {
		return err
	}
	fmt.Printf("next:  id=%d\n", id)

	b, err := c.generateBatch(name, 3, fmt.Sprintf("example-%d", time.Now().UnixNano()))
	if err != nil {
		return err
	}
	fmt.Printf("batch: first=%d last=%d count=%d step=%d\n", b.First, b.Last, b.Count, b.Step)

	l, err := c.last(name)
	if err != nil {
		return err
	}
	fmt.Printf("last:  last_id=%s next_id=%s\n", optional(l.LastID), optional(l.NextID))

	cols, err := c.listAll()
	if err != nil {
		return err
	}
	for _, col := range cols {
		fmt.Printf("list:  %s last_id=%s next_id=%s\n", col.Name, optional(col.LastID), optional(col.NextID))
	}
	return nil
}

func main() {
	key := os.Getenv("IDBSEQ_KEY")
	if key == "" {
		fmt.Fprintln(os.Stderr, "error: IDBSEQ_KEY is not set (an API key with read + generate scopes)")
		os.Exit(2)
	}
	c := &client{
		baseURL:    strings.TrimRight(envOr("IDBSEQ_URL", defaultURL), "/"),
		key:        key,
		httpClient: &http.Client{Timeout: timeout},
	}
	if err := run(c, envOr("IDBSEQ_COLLECTION", "examples")); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}
