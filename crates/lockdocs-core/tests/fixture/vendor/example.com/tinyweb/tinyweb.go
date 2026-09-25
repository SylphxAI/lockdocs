// Package tinyweb is a tiny web framework.
package tinyweb

// Context carries the request.
type Context struct{}

// JSON writes obj as JSON with the status code.
func (c *Context) JSON(code int, obj any) {}
