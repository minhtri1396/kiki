package mcp

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

// jsonObject keeps object keys in file order and stores each value unchanged.
type jsonObject struct {
	keys []string
	vals map[string]json.RawMessage
}

func newJSONObject() *jsonObject {
	return &jsonObject{vals: map[string]json.RawMessage{}}
}

func readObject(path string) (*jsonObject, bool, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return newJSONObject(), true, nil
		}
		return nil, false, err
	}
	if strings.TrimSpace(string(data)) == "" {
		return newJSONObject(), true, nil
	}
	obj, err := parseJSONObject(data)
	if err != nil {
		return nil, false, err
	}
	return obj, false, nil
}

func parseJSONObject(data []byte) (*jsonObject, error) {
	dec := json.NewDecoder(bytes.NewReader(bytes.TrimSpace(data)))
	tok, err := dec.Token()
	if err != nil {
		return nil, fmt.Errorf("not valid JSON")
	}
	delim, ok := tok.(json.Delim)
	if !ok || delim != '{' {
		return nil, fmt.Errorf("not a JSON object")
	}
	obj := newJSONObject()
	for dec.More() {
		tok, err = dec.Token()
		if err != nil {
			return nil, fmt.Errorf("not valid JSON")
		}
		key, ok := tok.(string)
		if !ok {
			return nil, fmt.Errorf("not a JSON object")
		}
		var val json.RawMessage
		if err := dec.Decode(&val); err != nil {
			return nil, fmt.Errorf("not valid JSON")
		}
		if _, exists := obj.vals[key]; !exists {
			obj.keys = append(obj.keys, key)
		}
		obj.vals[key] = val
	}
	tok, err = dec.Token()
	if err != nil || tok != json.Delim('}') {
		return nil, fmt.Errorf("not valid JSON")
	}
	if dec.More() {
		return nil, fmt.Errorf("not valid JSON")
	}
	return obj, nil
}

func (o *jsonObject) set(key string, raw []byte) {
	if _, exists := o.vals[key]; !exists {
		o.keys = append(o.keys, key)
	}
	o.vals[key] = json.RawMessage(bytes.TrimSpace(raw))
}

func (o *jsonObject) compact() []byte {
	var buf bytes.Buffer
	buf.WriteByte('{')
	for i, key := range o.keys {
		if i > 0 {
			buf.WriteByte(',')
		}
		encoded, _ := json.Marshal(key)
		buf.Write(encoded)
		buf.WriteByte(':')
		buf.Write(bytes.TrimSpace(o.vals[key]))
	}
	buf.WriteByte('}')
	return buf.Bytes()
}

func (o *jsonObject) pretty() []byte {
	var buf bytes.Buffer
	buf.WriteString("{\n")
	for i, key := range o.keys {
		encoded, _ := json.Marshal(key)
		fmt.Fprintf(&buf, "  %s: %s", encoded, bytes.TrimSpace(o.vals[key]))
		if i+1 < len(o.keys) {
			buf.WriteByte(',')
		}
		buf.WriteByte('\n')
	}
	buf.WriteString("}\n")
	return buf.Bytes()
}

func isNull(raw json.RawMessage) bool {
	return string(bytes.TrimSpace(raw)) == "null"
}
