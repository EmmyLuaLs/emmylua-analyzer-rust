# alias {{ type_name }}
---
{% if namespace %}
- namespace: {{ namespace }}
{% endif %}

{% if origin_type %}
{{ origin_type }}
{% endif %}

{% if description %}
{{ description }}
{% endif %}

