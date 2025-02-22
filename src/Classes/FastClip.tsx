import { v4 as uuidv4 } from 'uuid';

class FastClip {
    id: string;
    value: string;
    label: string;
    icon: string;
    colour: string;
    visible: boolean;
    clear_time: number | undefined;

    constructor(value: string, label: string, icon: string, colour:string = "red", visible: boolean=true, clear_time: number | undefined = undefined) {
        this.id = uuidv4(); // Generate a unique ID
        this.value = value;
        this.label = label;
        this.icon = icon;
        this.colour = colour;
        this.visible = visible;
        this.clear_time = clear_time;
    }

    toJSON() {
        return {
            id: this.id,
            value: this.value,
            label: this.label,
            icon: this.icon,
            colour: this.colour,
            visible: this.visible,
            clear_time: this.clear_time,
        };
    }
}

export default FastClip;
