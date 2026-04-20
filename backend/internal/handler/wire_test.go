package handler

import (
	"reflect"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/stretchr/testify/require"
)

func TestProvideAdminUserHandlerPassesTokenBillingService(t *testing.T) {
	tokenSvc := &service.ClientTokenBillingService{}

	h := ProvideAdminUserHandler(nil, nil, tokenSvc)

	require.NotNil(t, h)
	field := reflect.ValueOf(h).Elem().FieldByName("tokenBillingService")
	require.True(t, field.IsValid())
	require.Equal(t, reflect.ValueOf(tokenSvc).Pointer(), field.Pointer())
}
