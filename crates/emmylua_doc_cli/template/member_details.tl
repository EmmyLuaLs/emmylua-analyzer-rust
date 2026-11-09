{% macro details(member) -%}
{% if member.overloads %}
**Overloads**

{% for o in member.overloads %}{{ o }}
{% endfor %}
{% endif %}
{% if member.params %}
**Parameters**

{% for p in member.params %}- **{{ p.name }}**{% if p.type_text %} (*{{ p.type_text }}*){% endif %}{% if p.description %} - {{ p.description }}{% endif %}
{% endfor %}
{% endif %}
{% if member.returns %}
**Returns**

{% for r in member.returns %}- {% if r.name %}**{{ r.name }}** {% endif %}{% if r.type_text %}(*{{ r.type_text }}*){% endif %}{% if r.description %} - {{ r.description }}{% endif %}
{% endfor %}
{% endif %}
{%- endmacro details %}
