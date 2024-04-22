VERSION 0.8
IMPORT github.com/earthly/lib/rust AS rust

FROM rust:slim-buster

rust-build:
    DO rust+INIT --keep_fingerprints=true

# Builds the small example from redact-composer/examples
build-lib-example:
    FROM +rust-build
    WORKDIR /build-lib-example
    COPY --keep-ts --dir redact-composer/redact-composer redact-composer
    COPY --keep-ts --dir redact-composer/redact-composer-core redact-composer-core
    COPY --keep-ts --dir redact-composer/redact-composer-derive redact-composer-derive
    COPY --keep-ts --dir redact-composer/redact-composer-musical redact-composer-musical
    COPY --keep-ts --dir redact-composer/redact-composer-midi redact-composer-midi
    COPY --keep-ts redact-composer/Cargo.toml .
    COPY --keep-ts redact-composer/Cargo.lock .
    COPY --keep-ts redact-composer/README.md .

    DO rust+CARGO --args="build --release --package redact-composer --example simple" --output="release/examples/[^/\.]+"

    SAVE ARTIFACT ./target/release/examples/simple ./example

# Runs the small example from redact-composer/examples
run-lib-example:
    WORKDIR /run-lib-example
    COPY +build-lib-example/example .
    RUN "./example"
    SAVE ARTIFACT composition.mid
    SAVE ARTIFACT composition.json

# Generates the mp3 (.mp3) output from redact-composer/examples
gen-lib-example-mp3:
    FROM +timidity
    WORKDIR /mp3-lib-example
    COPY +run-lib-example/composition.mid .
    RUN timidity composition.mid -A18a,60a -Ow --output-24bit -o - | lame - output.mp3
    SAVE ARTIFACT output.mp3

# Generates the mp4 (.mp4) output from redact-composer/examples
# (useful for github readme)
gen-lib-example-mp4:
    FROM +ffmpeg
    WORKDIR /mp4-lib-example
    COPY +gen-lib-example-mp3/output.mp3 .
    RUN ffmpeg -f lavfi -i color=c=black:s=2x2:r=5 -i output.mp3 -c:v libx264 -c:a copy -shortest output.mp4
    SAVE ARTIFACT output.mp4

# gen-lib-example builds and run the redact-composer/example and provide various outputs.
# Args: --outputs : Comma separated list of formats to output (default: all)
#                   Possible values: all,midi,json,mp3,mp4
gen-lib-example:
    ENV output_options = "all,midi,json,mp3,mp4"
    ARG outputs = "all"
    FOR --sep "\t\n ," output IN $outputs
        IF [ "${output_options#*$output}" != "$output_options" ]
            IF [ $output = 'midi' -o $output = 'all' ]
                COPY +run-lib-example/composition.mid ./output.mid
                SAVE ARTIFACT output.mid
            END
            IF [ $output = 'json' -o $output = 'all' ]
                COPY +run-lib-example/composition.json ./output.json
                SAVE ARTIFACT output.json
            END
            IF [ $output = 'mp3' -o $output = 'all' ]
                COPY +gen-lib-example-mp3/output.mp3 .
                SAVE ARTIFACT output.mp3
            END
            IF [ $output = 'mp4' -o $output = 'all' ]
                COPY +gen-lib-example-mp4/output.mp4 .
                SAVE ARTIFACT output.mp4
            END
        ELSE
            RUN --no-cache echo Invalid output format: $output. Available options: $output_options
        END
    END

build-example:
    FROM +rust-build
    WORKDIR /build-example
    COPY --keep-ts --dir redact-composer/redact-composer redact-composer/redact-composer
    COPY --keep-ts --dir redact-composer/redact-composer-core redact-composer/redact-composer-core
    COPY --keep-ts --dir redact-composer/redact-composer-derive redact-composer/redact-composer-derive
    COPY --keep-ts --dir redact-composer/redact-composer-musical redact-composer/redact-composer-musical
    COPY --keep-ts --dir redact-composer/redact-composer-midi redact-composer/redact-composer-midi
    COPY --keep-ts redact-composer/Cargo.toml redact-composer/Cargo.toml
    COPY --keep-ts redact-composer/Cargo.lock redact-composer/Cargo.lock
    COPY --keep-ts redact-composer/README.md redact-composer/README.md
    COPY --keep-ts --dir redact-example/src redact-example/Cargo.toml redact-example
    WORKDIR ./redact-example

    DO rust+CARGO --args="build --release --package redact-example" --output="release/[^/\.]+"

    SAVE ARTIFACT ./target/release/redact-example

run-example:
    ARG RUST_LOG
    ARG CACHE_KEY=""

    FROM +timeable
    WORKDIR /run-example
    COPY +build-example/redact-example .
    ENV RUST_LOG=$RUST_LOG
    ENV RUST_BACKTRACE=1
    RUN --no-cache /usr/bin/time -v ./redact-example
    SAVE ARTIFACT test-midi/output.mid ./composition.mid
    SAVE ARTIFACT test-midi/output.json ./composition.json

gen-example-mp3:
    ARG CACHE_KEY=""

    FROM +timidity
    WORKDIR /mp3-example
    COPY (+run-example/composition.mid --CACHE_KEY=$CACHE_KEY) .
    RUN timidity composition.mid -A18a,60a -Ow --output-24bit -o - | lame - output.mp3
    SAVE ARTIFACT output.mp3

gen-example-mp4:
    ARG CACHE_KEY=""

    FROM +ffmpeg
    WORKDIR /mp4-example
    COPY (+gen-example-mp3/output.mp3 --CACHE_KEY=$CACHE_KEY) .
    RUN ffmpeg -f lavfi -i color=c=black:s=2x2:r=5 -i output.mp3 -c:v libx264 -crf 0 -c:a copy -shortest output.mp4
    SAVE ARTIFACT output.mp4

# gen-example builds and run redact-example and provide various outputs.
# Args: --outputs : Comma separated list of formats to output (default: all)
#                   Possible values: all,midi,json,mp3,mp4
gen-example:
    ENV output_options = "all,midi,json,mp3,mp4"
    ARG outputs = "all"
    ARG prefix = "output"
    ARG CACHE_KEY = $prefix
    FOR --sep "\t\n ," output IN $outputs
        IF [ "${output_options#*$output}" != "$output_options" ]
            IF [ $output = 'midi' -o $output = 'all' ]
                COPY (+run-example/composition.mid --CACHE_KEY=$prefix) ./$prefix.mid
                SAVE ARTIFACT $prefix.mid
            END
            IF [ $output = 'json' -o $output = 'all' ]
                COPY (+run-example/composition.json --CACHE_KEY=$prefix) ./$prefix.json
                SAVE ARTIFACT $prefix.json
            END
            IF [ $output = 'mp3' -o $output = 'all' ]
                COPY (+gen-example-mp3/output.mp3 --CACHE_KEY=$prefix) ./$prefix.mp3
                SAVE ARTIFACT $prefix.mp3
            END
            IF [ $output = 'mp4' -o $output = 'all' ]
                COPY (+gen-example-mp4/output.mp4 --CACHE_KEY=$prefix) ./$prefix.mp4
                SAVE ARTIFACT $prefix.mp4
            END
        ELSE
            RUN --no-cache echo Invalid output format: $output. Available options: $output_options
        END
    END

# random-10 Generates 10 random example outputs.
random-10:
    WORKDIR random-10
    FOR --sep " " random IN "random0 random1 random2 random3 random4 random5 random6 random7 random8 random9"
        COPY (+gen-example/* --prefix=$random) .
    END

    SAVE ARTIFACT ./*

timidity:
    RUN apt-get update && apt-get install -y \
        fluid-soundfont-gs \
        timidity \
        lame \
        ; \
        echo "source /etc/timidity/fluidr3_gs.cfg" >> /etc/timidity/timidity.cfg ; \
        apt-get autoremove -y

ffmpeg:
    RUN apt-get update && apt-get install -y ffmpeg ; \
        apt-get autoremove -y

timeable:
    RUN apt-get update && apt-get install -y time ; \
        apt-get autoremove -y