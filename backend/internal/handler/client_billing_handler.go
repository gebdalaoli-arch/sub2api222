package handler

import (
	"github.com/Wei-Shaw/sub2api/internal/pkg/response"
	middleware2 "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type ClientBillingHandler struct {
	tokenBillingService *service.ClientTokenBillingService
}

func NewClientBillingHandler(tokenBillingService *service.ClientTokenBillingService) *ClientBillingHandler {
	return &ClientBillingHandler{tokenBillingService: tokenBillingService}
}

func (h *ClientBillingHandler) GetSummary(c *gin.Context) {
	subject, ok := middleware2.GetAuthSubjectFromContext(c)
	if !ok {
		response.Unauthorized(c, "User not authenticated")
		return
	}
	summary, err := h.tokenBillingService.GetBillingSummary(c.Request.Context(), subject.UserID)
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, summary)
}
