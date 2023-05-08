from datetime import datetime, timezone


def date_to_utc_iso(date: datetime):
    return date.astimezone(timezone.utc).isoformat()
