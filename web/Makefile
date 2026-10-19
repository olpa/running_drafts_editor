IMAGE_TAG ?= 0.1.0
APPLICATION_IMAGE := running-drafts-frontend:$(IMAGE_TAG)

.PHONY: image test

image:
	docker build --pull --target runtime --tag $(APPLICATION_IMAGE) .

test:
	npm test
	npm run build
