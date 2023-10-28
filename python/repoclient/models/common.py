from pydantic import Field
from typing import Optional
from repoclient.models.base_model import ClientBaseModel
from datetime import datetime
from logging import getLogger

logger = getLogger("repoclient")

from repoclient.util import date_to_utc_iso


class UserFormatFilter(ClientBaseModel):
    format_id_eq: Optional[int] = Field(None, alias="formatIdEq")
    user_id_eq: Optional[int] = Field(None, alias="userId")
    created_at_eq: Optional[str] = Field(None, alias="createdAtEq")
    created_at_gte: Optional[str] = Field(None, alias="createdAtGte")
    created_at_lte: Optional[str] = Field(None, alias="createdAtLte")

    def user_id_equals(self, user_id: int):
        self.user_id_eq = user_id
        return self

    def format_id_equals(self, format_id: int):
        self.format_id_eq = format_id
        return self

    def created_after(self, date: datetime):
        self.created_at_gte = date_to_utc_iso(date)
        return self

    def created_before(self, date: datetime):
        self.created_at_lte = date_to_utc_iso(date)
        return self

    def created_on(self, date: datetime):
        self.created_at_eq = date_to_utc_iso(date)
        return self

    def build_url(self, url: str):
        for k, v in self.dict(by_alias=True, exclude_none=True).items():
            url += f"{k}={v}&"
        logger.debug("upstream url (including filters): %s", url)
        return url
