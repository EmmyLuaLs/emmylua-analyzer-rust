# global {{ doc.name }}

{% if doc.property.description %}
{{ doc.property.description }}
{% endif %}

{% if doc.property.deprecated %}
@deprecated {{ doc.property.deprecated }}
{% endif %}

{% if doc.property.see %}
@see {{ doc.property.see }}
{% endif %}

{% if doc.property.other %}
{{ doc.property.other }}
{% endif %}
---
{% if doc.methods %}
## methods
---
{% for method in doc.methods %}
### {{ method.name }}
---
{{ method.display }}

{% if method.overloads %}
**Overloads**

{% for o in method.overloads %}{{ o }}
{% endfor %}
{% endif %}
{% if method.params %}
**Parameters**

{% for p in method.params %}- **{{ p.name }}**{% if p.type_text %} (*{{ p.type_text }}*){% endif %}{% if p.description %} - {{ p.description }}{% endif %}
{% endfor %}
{% endif %}
{% if method.returns %}
**Returns**

{% for r in method.returns %}- {% if r.name %}**{{ r.name }}** {% endif %}{% if r.type_text %}(*{{ r.type_text }}*){% endif %}{% if r.description %} - {{ r.description }}{% endif %}
{% endfor %}
{% endif %}
{% if method.property.description %}
{{ method.property.description }}
{% endif %}

{% if method.property.deprecated %}
@deprecated {{ method.property.deprecated }}
{% endif %}

{% if method.property.see %}
@see {{ method.property.see }}
{% endif %}

{% if method.property.other %}
{{ method.property.other }}
{% endif %}
{% endfor %}
{% endif %}

{% if doc.fields %}
## fields
---
{% for field in doc.fields %}
### {{ field.name }}
---
{{ field.display }}

{% if field.overloads %}
**Overloads**

{% for o in field.overloads %}{{ o }}
{% endfor %}
{% endif %}
{% if field.params %}
**Parameters**

{% for p in field.params %}- **{{ p.name }}**{% if p.type_text %} (*{{ p.type_text }}*){% endif %}{% if p.description %} - {{ p.description }}{% endif %}
{% endfor %}
{% endif %}
{% if field.returns %}
**Returns**

{% for r in field.returns %}- {% if r.name %}**{{ r.name }}** {% endif %}{% if r.type_text %}(*{{ r.type_text }}*){% endif %}{% if r.description %} - {{ r.description }}{% endif %}
{% endfor %}
{% endif %}
{% if field.property.description %}
{{ field.property.description }}
{% endif %}

{% if field.property.deprecated %}
@deprecated {{ field.property.deprecated }}
{% endif %}

{% if field.property.see %}
@see {{ field.property.see }}
{% endif %}

{% if field.property.other %}
{{ field.property.other }}
{% endif %}
{% endfor %}
{% endif %}
