package schema

import (
	"github.com/Wei-Shaw/sub2api/ent/schema/mixins"

	"entgo.io/ent"
	"entgo.io/ent/dialect/entsql"
	"entgo.io/ent/schema"
	"entgo.io/ent/schema/field"
	"entgo.io/ent/schema/index"
)

// DesktopSession defines the schema for desktop runtime sessions.
type DesktopSession struct {
	ent.Schema
}

func (DesktopSession) Annotations() []schema.Annotation {
	return []schema.Annotation{
		entsql.Annotation{Table: "desktop_sessions"},
	}
}

func (DesktopSession) Mixin() []ent.Mixin {
	return []ent.Mixin{
		mixins.TimeMixin{},
	}
}

func (DesktopSession) Fields() []ent.Field {
	return []ent.Field{
		field.String("session_id").
			Unique(),
		field.Int64("user_id"),
		field.Int64("group_id"),
		field.String("device_id"),
		field.String("device_name").
			Default(""),
		field.String("target"),
		field.String("status").
			Default("active"),
		field.String("runtime_token_hash").
			Unique(),
		field.String("profile_key"),
		field.Time("expires_at"),
		field.Time("last_seen_at"),
		field.Time("revoked_at").
			Optional().
			Nillable(),
	}
}

func (DesktopSession) Indexes() []ent.Index {
	return []ent.Index{
		index.Fields("user_id"),
		index.Fields("group_id"),
		index.Fields("device_id"),
		index.Fields("expires_at"),
	}
}
