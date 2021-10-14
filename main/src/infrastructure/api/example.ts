import * as realearn from './realearn';
import {Mapping} from "./realearn";

const mapping: Mapping = {
    source: {
        type: "MidiDisplay",
        spec: {
            type: "MackieLcd",
            channel: 5,
            line: 2
        }
    }
}