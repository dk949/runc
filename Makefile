include config.mk
all: build

build: runc.rs
	rustc runc.rs -O
	strip runc

clean:
	rm -rf runc

install: all
	mkdir -p ${DESTDIR}${PREFIX}/bin/
	install runc ${DESTDIR}${PREFIX}/bin/

uninstall:
	rm -f ${DESTDIR}${PREFIX}/bin/runc

.PHONY: all clean install uninstall

