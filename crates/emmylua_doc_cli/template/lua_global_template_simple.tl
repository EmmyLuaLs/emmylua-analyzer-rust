{% import "member_details.tl" as member_details %}
# global {{ doc.name }}

---
{% if doc.display %}
{{ doc.display }}
{% endif %}
{{ member_details::details(member=doc) }}

{% if doc.property.description %}
{{ doc.property.description }}
{% endif %}
