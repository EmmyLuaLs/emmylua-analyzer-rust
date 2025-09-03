# Minimal makefile for Sphinx documentation
#

# You can set these variables from the command line, and also
# from the environment for the first two.
SPHINXOPTS    ?= -j auto -n
SPHINXBUILD   ?= sphinx-build
SOURCEDIR     = source
LANGUAGE      ?= en
BUILDDIR      = build

# Put it first so that "make" without argument is like "make help".
help:
	@$(SPHINXBUILD) -M help "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)

.PHONY: help html html-root html-all update-translation Makefile

html-root:
	mkdir -p build/html/
	cp -r "$(SOURCEDIR)/_root/." "$(BUILDDIR)/html/"

html: html-root
	@$(SPHINXBUILD) -b dirhtml "$(SOURCEDIR)" "$(BUILDDIR)/html/$(LANGUAGE)" -D language="$(LANGUAGE)" $(SPHINXOPTS) $(O)

html-all: html-root
	@$(SPHINXBUILD) -b dirhtml "$(SOURCEDIR)" "$(BUILDDIR)/html/en" -D language="en" $(SPHINXOPTS) $(O)
	@$(SPHINXBUILD) -b dirhtml "$(SOURCEDIR)" "$(BUILDDIR)/html/zh" -D language="zh" $(SPHINXOPTS) $(O)

update-translation:
	rm -rf "$(BUILDDIR)/gettext"
	@$(SPHINXBUILD) -M gettext "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)
	sphinx-intl update -p "$(BUILDDIR)/gettext" -l zh

%: Makefile
	@$(SPHINXBUILD) -M $@ "$(SOURCEDIR)" "$(BUILDDIR)" -D language="$(LANGUAGE)" $(SPHINXOPTS) $(O)
