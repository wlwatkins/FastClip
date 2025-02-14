import { v4 as uuidv4 } from 'uuid';

class FastClip {
    id: string;
    value: string;
    label: string;
    icon: string;
    clear_time: number;

    constructor(value: string, label: string, icon: string, clear_time: number) {
        this.id = uuidv4(); // Generate a unique ID
        this.value = value;
        this.label = label;
        this.icon = icon;
        this.clear_time = clear_time;
    }

    toJSON() {
        return {
            id: this.id,
            value: this.value,
            label: this.label,
            icon: this.icon,
            clear_time: this.clear_time,
        };
    }
}

export default FastClip;
