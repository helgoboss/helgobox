import {createSVGWindow} from "svgdom";
import svgjs from "@svgdotjs/svg.js";

const {SVG, registerWindow} = svgjs;
import * as fs from 'fs';
import * as path from 'path';
import pako from "pako";
import got from "got";
import '@svgdotjs/svg.topath.js';

const script_dir = path.resolve('doc/svg-gen');

// We need to embed the CSS into the SVG, otherwise the browser won't load it, tried it.
// (see https://stackoverflow.com/questions/18434094/how-to-style-svg-with-external-css).
const stylesheet = fs.readFileSync(path.resolve(script_dir, 'styles.css'));

generate()

async function generate() {
    fs.writeFileSync('doc/architecture/images/modules.svg', await generateModulesDiagram())
    fs.writeFileSync('doc/architecture/images/onion-layers.svg', generateOnionLayersDiagram())
    fs.writeFileSync('doc/architecture/images/components.svg', await generateComponentDiagram())
    fs.writeFileSync('doc/architecture/images/components-midi-fx.svg', await generateComponentDiagram([
        'role-realtime-general',
        'role-realtime-midi-general',
        'role-realtime-midi-fx'
    ]))
    fs.writeFileSync('doc/architecture/images/components-midi-device.svg', await generateComponentDiagram([
        'role-realtime-general',
        'role-realtime-midi-general',
        'role-realtime-midi-device'
    ]))
    fs.writeFileSync('doc/architecture/images/components-osc.svg', await generateComponentDiagram([
        'role-realtime-general',
        'role-realtime-osc',
    ]))
    fs.writeFileSync('doc/architecture/images/components-management.svg', await generateComponentDiagram([
        'role-sync-data',
        'role-notify',
    ]))
}

async function generateModulesDiagram() {
    const draw = await loadDotToSvgCanvas('modules.dot');
    return draw.svg();
}

async function generateComponentDiagram(shownEdgeClasses = undefined) {
    const draw = await loadDotToSvgCanvas('components.dot');
    if (shownEdgeClasses) {
        draw.style('.edge', {visibility: 'hidden'})
        for (let edgeClass of shownEdgeClasses) {
            draw.style(`.edge.${edgeClass}`, {visibility: 'visible'})
        }
    }
    return draw.svg();
}

function generateOnionLayersDiagram() {
    const draw = initSvgCanvas();
    const width = 410;
    const height = 410;
    draw.size(width, height);
    embedStylesheet(draw);

    // Default attributes
    const defaultFontSize = 15;
    const defaultArrowColor = 'black';
    const defaultArrowWidth = 2;
    const baseFont = {
        size: defaultFontSize,
    }
    const arrowPattern = createArrowPattern('rule-arrow');
    const defaultArrowHead = arrowHead();

    // Layers
    const infrastructureLayer = layer(4, 'layer-infrastructure', 'infrastructure', [
        'GUI',
        'API',
        'Persistence',
        'Server',
    ]);
    layer(3, 'layer-management', 'management');
    layer(2, 'layer-processing', 'processing');
    const baseLayer = layer(1, 'layer-base', 'base');

    // Arrows
    drawArrow(infrastructureLayer.x(), infrastructureLayer.cy(), baseLayer.cx(), baseLayer.cy(), {
        text: 'may use code in',
        patternOrColor: arrowPattern,
        width: 10,
        drawHead: false,
        cssClass: 'rule-arrow',
        useClipping: true,
    });

    function arrowHead() {
        return draw.marker(10, 7, (add) => {
            add.polygon('0,0 10,3.5 0,7').addClass('arrow-head');
        });
    }

    function arcPath(radius, cx, cy, sweep) {
        return [
            // Start at 9
            ['M', cx - radius, cy],
            // Go to 3
            ['A', -radius, -radius, 0, 0, sweep, cx + radius, cy],
        ];
    }

    function layer(index, cssClass, label, components = []) {
        const g = draw.group().addClass(cssClass);
        const spacing = 50;
        const radius = index * spacing;
        const circle = g
            .circle(radius * 2)
            .center(width / 2, height / 2)
            .fill('none')
            .stroke({color: 'black'})
            .addClass('layerpart-circle')
        const pathRadius = radius - spacing / 2;
        const radiusFix = defaultFontSize / 3;
        const upperArc = arcPath(
            pathRadius - radiusFix,
            circle.cx(),
            circle.cy(),
            1
        );
        g.textPath(label, upperArc)
            .attr('text-anchor', 'middle')
            .attr('startOffset', '50%')
            .font(baseFont)
            .addClass('layerpart-label')
        const lowerArc = arcPath(
            pathRadius + radiusFix,
            circle.cx(),
            circle.cy(),
            0
        );
        for (let i = 0; i < components.length; i++) {
            const segmentLength = (1 / components.length) * 100;
            const offset = i * segmentLength + segmentLength / 2;
            g.textPath(components[i], lowerArc)
                .attr('text-anchor', 'middle')
                .attr('startOffset', `${offset}%`)
                .font(baseFont)
                .addClass('layerpart-secondary-label');
        }
        return circle;
    }

    function createArrowPattern(cssClass = undefined) {
        return draw.pattern(30, 20, (add) => {
            const g = add.group().addClass(cssClass);
            g.line(0, 3.5, 10, 3.5)
                .stroke({color: defaultArrowColor, width: 1})
                .addClass('arrow-pattern-path');
            g.polygon('10,0 20,3.5 10,7').addClass('arrow-pattern-head');
        });
    }

    function drawArrow(x1, y1, x2, y2, {
        patternOrColor = defaultArrowColor,
        width = defaultArrowWidth,
        head = defaultArrowHead,
        text,
        drawHead = true,
        cssClass = undefined,
        useClipping = false,
    }) {
        // Group for clipping
        const outer_group = draw.group().addClass(cssClass);
        const inner_group = outer_group.group();
        // Arrow itself
        const line = useClipping ? inner_group.line(x1 - 100, y1, x2 + 100, y2) : inner_group.line(x1, y1, x2, y2);
        const path = line.toPath().addClass('arrow-path');
        path.stroke({color: patternOrColor, width});
        // Head
        if (drawHead) {
            path.marker('end', head.size(8, 8))
        }
        // Text
        outer_group.textPath()
            .plot(path.array())
            .text(add => {
                add.tspan(text).dy(-10)
            })
            .addClass('arrowpart-label')
            .font({...baseFont, anchor: 'middle', startOffset: '50%'});
        // Clip (useful for keeping CSS transform animation within bounds)
        if (useClipping) {
            const clip = inner_group.clip()
                .add(inner_group.polygon().plot([[x1, y1 - width], [x2, y2 - width], [x2, y1 + width], [12, y2 + width]]));
            inner_group.clipWith(clip);
        }
        return outer_group;
    }

    return draw.svg();
}

async function loadDotToSvgCanvas(dotFileName) {
    const dot = fs.readFileSync(path.resolve(script_dir, dotFileName));
    const svg = await convertDotToSvg(dot);
    const qualifiedSvg = svg.replace(/<(!DOCTYPE[^>\[]+(\[[^\]]+)?)+[^>]+>/g, '');
    const window = createSVGWindow()
    const document = window.document
    registerWindow(window, document)
    const originalDraw = SVG(document.documentElement);
    originalDraw.svg(qualifiedSvg, true);
    const draw = SVG(document.documentElement);
    embedStylesheet(draw);
    return draw;
}

async function convertDotToSvg(dot) {
    const data = Buffer.from(dot, 'utf8')
    const compressed = pako.deflate(data, {level: 9})
    const body = Buffer.from(compressed)
        .toString('base64')
        .replace(/\+/g, '-').replace(/\//g, '_')
    const response = await got(`https://kroki.io/graphviz/svg/${body}`);
    return response.body
}

function initSvgCanvas() {
    const window = createSVGWindow()
    const document = window.document
    registerWindow(window, document)
    return SVG(document.documentElement);
}

function embedStylesheet(draw) {
    draw.element('style').words(stylesheet)
}