## Earthly Branch
Contains an `Earthfile` used to quickly generate example outputs and convert the midi output to mp3.

## Usage
0. Install [Earthly](https://earthly.dev/get-earthly).
1. Copy the `Earthfile` to the parent of [`redact-renderer-example`](https://github.com/dousto/redact-renderer-example).
2. In the `Earthfile` directory, run `earthly doc` to see available commands.

### Example
```shell
earthly -a +gen-example/\* --outputs=json,midi,mp3 ./example-outputs/ --RUST_LOG=debug
```